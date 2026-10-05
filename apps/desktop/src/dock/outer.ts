import type { DockviewApi } from 'dockview-vue';
import { ref, shallowRef } from 'vue';
import { LEGACY_KEY, OUTER_KEY, TOOL_PANES, missingPanes, parseOuter, toggleAction, type PaneState, type ToolPane } from './outerModel';
import { read, remove, write } from './storage';

const TITLES: Record<ToolPane, string> = { projects: 'Projects', workspace: 'Workspace', diff: 'Diff', activity: 'Activity Monitor' };

const outer = shallowRef<DockviewApi | null>(null);
export const layoutVersion = ref(0);
let closed: ToolPane[] = [];
let disposables: { dispose(): void }[] = [];

function addDefault(api: DockviewApi, pane: ToolPane) {
  const base = { id: pane, component: pane, title: TITLES[pane] };
  const workspace = api.getPanel('workspace');
  if (pane === 'workspace') api.addPanel({ ...base, tabComponent: 'fixed' });
  else if (pane === 'projects') api.addPanel({ ...base, initialWidth: 260, ...(workspace && { position: { referencePanel: workspace, direction: 'left' } }) });
  else if (pane === 'diff') api.addPanel({ ...base, initialWidth: 420, ...(workspace && { position: { referencePanel: workspace, direction: 'right' } }) });
  else {
    const diff = api.getPanel('diff');
    if (diff) api.addPanel({ ...base, inactive: true, position: { referencePanel: diff, direction: 'within' } });
    else api.addPanel({ ...base, initialWidth: 420, ...(workspace && { position: { referencePanel: workspace, direction: 'right' } }) });
  }
}

function save(api: DockviewApi) {
  write(OUTER_KEY, JSON.stringify({ layout: api.toJSON(), closed }));
}

export function attachOuter(api: DockviewApi) {
  remove(LEGACY_KEY);
  const stored = parseOuter(read(OUTER_KEY));
  if (stored) {
    try {
      api.fromJSON(stored.layout);
      closed = stored.closed;
    } catch {
      api.clear();
      closed = [];
    }
  }
  for (const pane of missingPanes(api.panels.map((p) => p.id), closed)) addDefault(api, pane);
  disposables = [
    api.onWillShowOverlay((e) => { if (e.getData()?.viewId !== api.id) e.preventDefault(); }),
    api.onDidRemovePanel((p) => {
      const pane = p.id as ToolPane;
      if (pane !== 'workspace' && !closed.includes(pane)) closed.push(pane);
    }),
    api.onDidAddPanel((p) => { closed = closed.filter((c) => c !== p.id); }),
    api.onDidLayoutChange(() => {
      save(api);
      layoutVersion.value++;
    }),
  ];
  outer.value = api;
  save(api);
  layoutVersion.value++;
}

export function detachOuter() {
  disposables.forEach((d) => d.dispose());
  disposables = [];
  outer.value = null;
}

export function paneState(pane: ToolPane): PaneState {
  void layoutVersion.value;
  const panel = outer.value?.getPanel(pane);
  if (!panel) return 'closed';
  return panel.group.activePanel === panel ? 'front' : 'background';
}

export function togglePane(pane: Exclude<ToolPane, 'workspace'>) {
  const api = outer.value;
  if (!api) return;
  const action = toggleAction(paneState(pane));
  if (action === 'open') addDefault(api, pane);
  else if (action === 'activate') api.getPanel(pane)?.api.setActive();
  else api.getPanel(pane)?.api.close();
}

export function resetOuterLayout() {
  const api = outer.value;
  if (!api) return;
  api.clear();
  closed = [];
  TOOL_PANES.forEach((pane) => addDefault(api, pane));
}
