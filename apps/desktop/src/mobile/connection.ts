import { ref } from 'vue';
import { errorMessage, setTransport } from '../api';
import { applyEvent, isConnected, refresh, state, toast } from '../store';
import type { NodeEvent, NodeStatus } from '../types';
import { adoptPairing, clearToken, socketUrl, tokenFromHash } from './pairing';
import { WsTransport } from './wsTransport';

export const paired = ref(false);
export const revoked = ref(false);
let transport: WsTransport | null = null;

function onStatus(status: NodeStatus) {
  const wasConnected = isConnected(state.node);
  state.node = status;
  if (!wasConnected && isConnected(status)) refresh().catch((e) => toast(errorMessage(e)));
}

function onEvent(event: NodeEvent) {
  // Archiving dropped the task's sessions from the store; a restore needs them back.
  const restored =
    event.method === 'task.changed' &&
    !event.params.archived &&
    !state.tasks.some((t) => t.id === event.params.id);
  applyEvent(state, event);
  if (restored) refresh().catch(() => {});
}

export function connect(token: string | null) {
  transport?.stop();
  transport = null;
  paired.value = token !== null;
  if (!token) return;
  revoked.value = false;
  transport = new WsTransport({
    url: socketUrl(location),
    token,
    onStatus,
    onEvent,
    onUnauthorized: () => {
      clearToken(localStorage);
      paired.value = false;
      revoked.value = true;
    },
  });
  setTransport(transport);
  transport.start();
}

document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'visible') transport?.wake();
});
window.addEventListener('online', () => transport?.wake());

window.addEventListener('hashchange', () => {
  const token = tokenFromHash(location.hash);
  if (!token) return;
  connect(adoptPairing(location, history, localStorage));
  // replaceState does not fire hashchange; the router needs to see "#/".
  window.dispatchEvent(new HashChangeEvent('hashchange'));
});
