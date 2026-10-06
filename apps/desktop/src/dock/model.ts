import type { SerializedDockview } from 'dockview-vue';

export type ToolPane = 'diff' | 'activity';
export const isToolPane = (id: string): id is ToolPane => id === 'diff' || id === 'activity';

export const WORKSPACE_PREFIX = 'asterism.workspace.';
export const FLOAT_PREFIX = 'asterism.float.';
export const TEMPLATE_KEY = 'asterism.layoutTemplate';
export const SIDEBAR_KEY = 'asterism.sidebar';
export const LEGACY_KEYS = ['asterism.layout', 'asterism.layout.v2'];

export const workspaceKey = (taskId: number) => `${WORKSPACE_PREFIX}${taskId}`;
export const floatKey = (taskId: number) => `${FLOAT_PREFIX}${taskId}`;
export const sessionPanelId = (sessionId: number) => `session-${sessionId}`;

export function sessionIdOf(panelId: string): number | null {
  const match = /^session-(\d+)$/.exec(panelId);
  return match ? Number(match[1]) : null;
}

export const filePanelId = (taskId: number, path: string) => `file:${taskId}:${path}`;

export function fileTaskOf(panelId: string): number | null {
  const match = /^file:(\d+):/.exec(panelId);
  return match ? Number(match[1]) : null;
}

/** Panels to drop and sessions to add so a saved layout shows exactly the task's sessions; tool panes and the task's own file panes stay. */
export function reconcile(panelIds: string[], sessionIds: number[], taskId: number): { remove: string[]; add: number[] } {
  const wanted = new Set(sessionIds);
  const shown = new Set<number>();
  const remove = panelIds.filter((id) => {
    if (isToolPane(id)) return false;
    // File panes inherited from another task through the layout template are dropped.
    const fileTask = fileTaskOf(id);
    if (fileTask !== null) return fileTask !== taskId;
    const session = sessionIdOf(id);
    if (session === null || !wanted.has(session)) return true;
    shown.add(session);
    return false;
  });
  return { remove, add: sessionIds.filter((id) => !shown.has(id)) };
}

export type PaneState = 'closed' | 'background' | 'front';

export function toggleAction(state: PaneState): 'open' | 'activate' | 'close' {
  return state === 'closed' ? 'open' : state === 'background' ? 'activate' : 'close';
}

export function parseWorkspace(raw: string | null): SerializedDockview | null {
  try {
    const value: unknown = JSON.parse(raw ?? 'null');
    const isLayout = typeof value === 'object' && value !== null && !Array.isArray(value) && typeof (value as { grid?: unknown }).grid === 'object';
    return isLayout ? (value as SerializedDockview) : null;
  } catch {
    return null;
  }
}

export function staleKeys(keys: string[], taskIds: number[]): string[] {
  const live = new Set(taskIds.flatMap((id) => [workspaceKey(id), floatKey(id)]));
  return keys.filter((key) => (key.startsWith(WORKSPACE_PREFIX) || key.startsWith(FLOAT_PREFIX)) && !live.has(key));
}

export type Placement = { referenceGroup: string; direction: 'within' | 'left' | 'right' | 'below' };

/** Where a new session goes: the requested spot, else the focused group, a session group, or left of the tools. */
export function placementPosition(
  placement: Placement | undefined,
  groups: { id: string; hasSession: boolean }[],
  lastFocused: string | null,
): Placement | undefined {
  const ids = groups.map((g) => g.id);
  if (placement && ids.includes(placement.referenceGroup)) return placement;
  if (lastFocused !== null && ids.includes(lastFocused)) return { referenceGroup: lastFocused, direction: 'within' };
  const sessionGroup = groups.find((g) => g.hasSession);
  if (sessionGroup) return { referenceGroup: sessionGroup.id, direction: 'within' };
  return groups[0] ? { referenceGroup: groups[0].id, direction: 'left' } : undefined;
}

export interface GroupSize { id: string; width: number; height: number }

/** Sizes to restore after a removal, which dockview follows by spreading space evenly; the largest group absorbs it. */
export function keptSizes(before: GroupSize[], remaining: string[]): GroupSize[] {
  const surviving = before.filter((g) => remaining.includes(g.id));
  const largest = surviving.reduce<GroupSize | null>((big, g) => (!big || g.width * g.height > big.width * big.height ? g : big), null);
  return surviving.filter((g) => g !== largest);
}

export const SIDEBAR = { initial: 260, min: 180, max: 480 };

export const clampWidth = (width: number) => Math.round(Math.min(SIDEBAR.max, Math.max(SIDEBAR.min, width)));

export function parseSidebar(raw: string | null): { width: number; open: boolean } {
  try {
    const v = JSON.parse(raw ?? '{}') as { width?: unknown; open?: unknown };
    return {
      width: typeof v.width === 'number' ? clampWidth(v.width) : SIDEBAR.initial,
      open: typeof v.open === 'boolean' ? v.open : true,
    };
  } catch {
    return { width: SIDEBAR.initial, open: true };
  }
}
