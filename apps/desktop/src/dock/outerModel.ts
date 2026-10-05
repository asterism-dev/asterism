import type { SerializedDockview } from 'dockview-vue';

export type ToolPane = 'projects' | 'workspace' | 'diff' | 'activity';
export const TOOL_PANES: ToolPane[] = ['workspace', 'projects', 'diff', 'activity'];
export const OUTER_KEY = 'asterism.layout.v2';
export const LEGACY_KEY = 'asterism.layout';

export type PaneState = 'closed' | 'background' | 'front';

export function toggleAction(state: PaneState): 'open' | 'activate' | 'close' {
  return state === 'closed' ? 'open' : state === 'background' ? 'activate' : 'close';
}

export interface StoredOuter { layout: SerializedDockview; closed: ToolPane[] }

const CLOSABLE: ToolPane[] = ['projects', 'diff', 'activity'];

export function isLayout(value: unknown): value is SerializedDockview {
  return typeof value === 'object' && value !== null && !Array.isArray(value) && typeof (value as { grid?: unknown }).grid === 'object';
}

export function parseOuter(raw: string | null): StoredOuter | null {
  try {
    const value = JSON.parse(raw ?? 'null') as { layout?: unknown; closed?: unknown } | null;
    if (!value || !isLayout(value.layout)) return null;
    const closed = Array.isArray(value.closed) ? value.closed.filter((p): p is ToolPane => CLOSABLE.includes(p as ToolPane)) : [];
    return { layout: value.layout, closed };
  } catch {
    return null;
  }
}

/** Panes to add after loading: absent and not closed on purpose; the Workspace always. */
export function missingPanes(present: string[], closed: ToolPane[]): ToolPane[] {
  return TOOL_PANES.filter((p) => !present.includes(p) && (p === 'workspace' || !closed.includes(p)));
}
