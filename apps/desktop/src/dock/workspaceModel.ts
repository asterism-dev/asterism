import type { SerializedDockview } from 'dockview-vue';
import { isLayout } from './outerModel';

export const WORKSPACE_PREFIX = 'asterism.workspace.';
export const workspaceKey = (taskId: number) => `${WORKSPACE_PREFIX}${taskId}`;
export const sessionPanelId = (sessionId: number) => `session-${sessionId}`;

export function sessionIdOf(panelId: string): number | null {
  const match = /^session-(\d+)$/.exec(panelId);
  return match ? Number(match[1]) : null;
}

export function reconcile(panelIds: string[], sessionIds: number[]): { remove: string[]; add: number[] } {
  const wanted = new Set(sessionIds);
  const shown = new Set<number>();
  const remove = panelIds.filter((id) => {
    const session = sessionIdOf(id);
    if (session === null || !wanted.has(session)) return true;
    shown.add(session);
    return false;
  });
  return { remove, add: sessionIds.filter((id) => !shown.has(id)) };
}

export function targetGroup(lastFocused: string | null, groupIds: string[]): string | undefined {
  return lastFocused !== null && groupIds.includes(lastFocused) ? lastFocused : groupIds[0];
}

export function parseWorkspace(raw: string | null): SerializedDockview | null {
  try {
    const value: unknown = JSON.parse(raw ?? 'null');
    return isLayout(value) ? value : null;
  } catch {
    return null;
  }
}

export function staleWorkspaceKeys(keys: string[], taskIds: number[]): string[] {
  const live = new Set(taskIds.map(workspaceKey));
  return keys.filter((key) => key.startsWith(WORKSPACE_PREFIX) && !live.has(key));
}
