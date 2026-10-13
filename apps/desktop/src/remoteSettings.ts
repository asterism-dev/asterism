import type { RemoteInterface, RemoteStatus } from './types';

export function interfaceLabel(i: RemoteInterface): string {
  if (i.tailscale) return `Tailscale · ${i.address}`;
  if (i.address === '0.0.0.0') return `All interfaces · ${i.address}`;
  return `${i.name} · ${i.address}`;
}

export function bindWarning(bind: string): string | null {
  return bind === '0.0.0.0' || bind === '::'
    ? 'Listens on every network this Mac joins, not only your tailnet.'
    : null;
}

export function statusLine(s: RemoteStatus): string {
  if (s.state === 'listening') return `Listening on ${s.address ?? ''}`;
  if (s.state === 'error') return `Not listening: ${s.error ?? 'unknown error'}`;
  return 'Off';
}
