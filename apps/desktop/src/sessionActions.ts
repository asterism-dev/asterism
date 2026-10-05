import { ask } from '@tauri-apps/plugin-dialog';
import { api, errorMessage } from './api';
import { placeNext, type Placement } from './dock/main';
import { addSession, sessionLabel, showMenu, state, toast, type MenuItem } from './store';
import type { Session, SessionKind } from './types';

const report = (e: unknown) => toast(errorMessage(e));
// Sessions being closed; a second click during the confirm or the daemon's grace period is ignored.
const closing = new Set<number>();

export function startSession(taskId: number, kind: SessionKind, placement?: Placement) {
  api.startSession(taskId, kind).then((s) => {
    if (placement) placeNext(s.id, placement);
    addSession(state, s);
  }).catch(report);
}

export function newSessionMenu(e: MouseEvent, taskId: number, groupId?: string, extra: MenuItem[] = [], floating = false) {
  const agents = state.agents.filter((a) => a.available);
  const shell: SessionKind = { type: 'shell' };
  const here: Placement | undefined = groupId ? { referenceGroup: groupId, direction: 'within' } : undefined;
  showMenu(e, [
    ...agents.map((a) => ({ label: a.display_name || a.name, action: () => startSession(taskId, { type: 'agent', name: a.name }, here) })),
    { label: 'Terminal', action: () => startSession(taskId, shell, here) },
    // A floating window holds a single group, so there is no below/right inside it.
    ...(groupId && !floating ? [
      { label: 'Terminal below', action: () => startSession(taskId, shell, { referenceGroup: groupId, direction: 'below' }) },
      { label: 'Terminal right', action: () => startSession(taskId, shell, { referenceGroup: groupId, direction: 'right' }) },
    ] : []),
    ...extra,
  ]);
}

export async function closeSession(s: Session) {
  if (closing.has(s.id)) return;
  closing.add(s.id);
  try {
    if (s.kind.type === 'agent' && s.status !== 'exited') {
      const stop = await ask(`Stop the running ${sessionLabel(s)} session and close it?`, { title: 'Close session', kind: 'warning' });
      if (!stop) return;
    }
    await api.removeSession(s.id);
  } catch (e) {
    report(e);
  } finally {
    closing.delete(s.id);
  }
}
