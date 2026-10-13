import { RpcError, type Transport } from '../api';
import type { HelloResult, NodeEvent, NodeStatus, SessionAttachResult } from '../types';

// Must match asterism_proto::PROTO_VERSION; the daemon serves this page, so a mismatch means a stale page.
export const PROTO_VERSION = 2;
export const UNAUTHORIZED = 4401;
const MIN_DELAY_MS = 1000;
const MAX_DELAY_MS = 30_000;

export interface SocketLike {
  readonly readyState: number;
  onopen: (() => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onclose: ((ev: { code: number; reason: string }) => void) | null;
  send(data: string): void;
  close(code?: number, reason?: string): void;
}

export interface WsTransportOptions {
  url: string;
  token: string;
  createSocket?: (url: string) => SocketLike;
  onStatus: (status: NodeStatus) => void;
  onEvent: (event: NodeEvent) => void;
  onUnauthorized: () => void;
}

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
}

const browserSocket = (url: string) => new WebSocket(url) as unknown as SocketLike;

export class WsTransport implements Transport {
  private socket: SocketLike | null = null;
  private ready = false;
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  private readonly outputs = new Map<number, (data: string) => void>();
  private delay = MIN_DELAY_MS;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private stopped = false;

  constructor(private readonly options: WsTransportOptions) {}

  start() {
    this.connect();
  }

  stop() {
    this.stopped = true;
    clearTimeout(this.timer);
    this.socket?.close();
  }

  wake() {
    if (this.stopped || this.socket) return;
    clearTimeout(this.timer);
    this.delay = MIN_DELAY_MS;
    this.connect();
  }

  call<T>(method: string, params: Record<string, unknown>): Promise<T> {
    if (!this.ready)
      return Promise.reject(new RpcError('disconnected', 'Not connected to the daemon'));
    return this.request<T>(method, params);
  }

  async attach(sessionId: number, onOutput: (data: string) => void) {
    // Registered first: output can arrive before the attach response.
    this.outputs.set(sessionId, onOutput);
    try {
      return await this.call<SessionAttachResult>('session.attach', { session_id: sessionId });
    } catch (e) {
      this.outputs.delete(sessionId);
      throw e;
    }
  }

  async detach(sessionId: number) {
    this.outputs.delete(sessionId);
    if (this.ready) await this.call<null>('session.detach', { session_id: sessionId });
  }

  private request<T>(method: string, params: Record<string, unknown>): Promise<T> {
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
      this.socket?.send(JSON.stringify({ jsonrpc: '2.0', id, method, params }));
    });
  }

  private connect() {
    this.options.onStatus({ state: 'connecting' });
    const socket = (this.options.createSocket ?? browserSocket)(this.options.url);
    this.socket = socket;
    socket.onopen = () => {
      socket.send(JSON.stringify({ auth: this.options.token }));
      void this.handshake(socket);
    };
    socket.onmessage = (ev) => this.onMessage(String(ev.data));
    socket.onclose = (ev) => this.onClose(socket, ev.code, ev.reason);
  }

  private async handshake(socket: SocketLike) {
    try {
      const hello = await this.request<HelloResult>('hello', {
        proto_version: PROTO_VERSION,
        client_kind: 'app',
      });
      await this.request<null>('subscribe', {});
      if (socket !== this.socket) return;
      this.ready = true;
      this.delay = MIN_DELAY_MS;
      this.options.onStatus({ state: 'connected', hello });
    } catch (e) {
      if (e instanceof RpcError && e.kind === 'incompatible_version') {
        this.stopped = true;
        this.options.onStatus({ state: 'incompatible', message: e.message });
        socket.close();
      } else if (socket === this.socket) {
        socket.close();
      }
    }
  }

  private onMessage(text: string) {
    let msg: {
      id?: number;
      result?: unknown;
      error?: { message: string; data?: { kind?: string } };
      method?: string;
      params?: { session_id?: number; data?: string };
    };
    try {
      msg = JSON.parse(text);
    } catch {
      return;
    }
    if (typeof msg.id === 'number') {
      const pending = this.pending.get(msg.id);
      if (!pending) return;
      this.pending.delete(msg.id);
      if (msg.error)
        pending.reject(new RpcError(msg.error.data?.kind ?? 'internal', msg.error.message));
      else pending.resolve(msg.result ?? null);
      return;
    }
    if (msg.method === 'session.output') {
      const { session_id, data } = msg.params ?? {};
      if (session_id !== undefined && data !== undefined) this.outputs.get(session_id)?.(data);
      return;
    }
    if (typeof msg.method === 'string')
      this.options.onEvent({ method: msg.method, params: msg.params } as NodeEvent);
  }

  private onClose(socket: SocketLike, code: number, reason: string) {
    if (socket !== this.socket) return;
    this.socket = null;
    this.ready = false;
    for (const pending of this.pending.values())
      pending.reject(new RpcError('disconnected', 'Connection to the daemon closed'));
    this.pending.clear();
    this.outputs.clear();
    if (code === UNAUTHORIZED) {
      this.stopped = true;
      this.options.onUnauthorized();
      return;
    }
    if (this.stopped) return;
    this.options.onStatus({ state: 'disconnected', reason: reason || 'Connection lost' });
    this.timer = setTimeout(() => this.connect(), this.delay);
    this.delay = Math.min(this.delay * 2, MAX_DELAY_MS);
  }
}
