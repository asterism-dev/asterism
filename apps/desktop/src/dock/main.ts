import type { DockviewApi, DockviewGroupPanel } from 'dockview-vue';
import { ref, shallowRef } from 'vue';
import { api, errorMessage } from '../api';
import { toast } from '../store';
import type { PanelInfo } from '../types';
import {
  LEGACY_KEYS,
  TEMPLATE_KEY,
  filePanelId,
  fileTaskOf,
  floatKey,
  keptSizes,
  pluginPanelId,
  sessionIdOf,
  staleKeys,
  toggleAction,
  workspaceKey,
  type GroupSize,
  type PaneState,
  type Placement,
  type ToolPane,
} from './model';
import { keys, read, remove, write } from './storage';

export type { Placement };

const TITLES: Record<ToolPane, string> = { diff: 'Diff', activity: 'Activity Monitor' };

// A session's placement is chosen before the daemon returns its id, so it waits here until its panel is added.
const placements = new Map<number, Placement>();

export const mainApi = shallowRef<DockviewApi | null>(null);
export const layoutVersion = ref(0);
export const layoutEpoch = ref(0);
export const floatUnlocked = ref(false);
let currentTask: number | null = null;

export function placeNext(sessionId: number, placement: Placement) {
  placements.set(sessionId, placement);
}

export function takePlacement(sessionId: number): Placement | undefined {
  const placement = placements.get(sessionId);
  placements.delete(sessionId);
  return placement;
}

const gridGroups = (api: DockviewApi) => api.groups.filter((g) => g.api.location.type === 'grid');
const hasSession = (g: DockviewGroupPanel) => g.panels.some((p) => sessionIdOf(p.id) !== null);

/** Adds a tool pane into `group`, else Activity behind Diff, else right of the sessions. */
export function addTool(
  api: DockviewApi,
  pane: ToolPane,
  options: { background?: boolean; group?: string } = {},
) {
  const base = {
    id: pane,
    component: pane,
    tabComponent: 'pane',
    title: TITLES[pane],
    inactive: options.background,
  };
  const diff = api.getPanel('diff');
  if (options.group)
    api.addPanel({ ...base, position: { referenceGroup: options.group, direction: 'within' } });
  else if (pane === 'activity' && diff)
    api.addPanel({ ...base, position: { referencePanel: diff, direction: 'within' } });
  else {
    const grid = gridGroups(api);
    const anchor = grid.find(hasSession) ?? grid[0];
    api.addPanel({
      ...base,
      initialWidth: 420,
      ...(anchor && { position: { referenceGroup: anchor, direction: 'right' } }),
    });
  }
}

/** Adds a plugin panel into `group`, else right of the sessions; an open one is brought to the front. */
export function addPluginPanel(
  api: DockviewApi,
  plugin: string,
  panel: PanelInfo,
  options: { group?: string } = {},
) {
  const id = pluginPanelId(plugin, panel.id);
  const existing = api.getPanel(id);
  if (existing) return existing.api.setActive();
  const base = {
    id,
    component: 'plugin',
    tabComponent: 'pane',
    title: panel.title,
    params: { plugin, panel: panel.id },
  };
  if (options.group)
    api.addPanel({ ...base, position: { referenceGroup: options.group, direction: 'within' } });
  else {
    const grid = gridGroups(api);
    const anchor = grid.find(hasSession) ?? grid[0];
    api.addPanel({
      ...base,
      initialWidth: 420,
      ...(anchor && { position: { referenceGroup: anchor, direction: 'right' } }),
    });
  }
}

/** Opens a file pane for `path`, or brings an open one to the front at `line`; unreadable files only toast. */
export async function openFile(taskId: number, path: string, line?: number) {
  const dock = mainApi.value;
  if (!dock) return;
  let shown: string;
  try {
    // ponytail: the pane reads the file again; hand the content over if large files feel slow to open.
    shown = (await api.file(taskId, path)).path;
  } catch (e) {
    toast(errorMessage(e));
    return;
  }
  const params = { taskId, path: shown, line };
  const existing = dock.getPanel(filePanelId(taskId, shown));
  if (existing) {
    existing.api.setActive();
    existing.api.updateParameters(params);
    return;
  }
  const base = {
    id: filePanelId(taskId, shown),
    component: 'file',
    tabComponent: 'pane',
    title: shown.slice(shown.lastIndexOf('/') + 1),
    params,
  };
  const sibling = dock.panels.find((p) => fileTaskOf(p.id) !== null);
  if (sibling) {
    dock.addPanel({ ...base, position: { referenceGroup: sibling.group.id, direction: 'within' } });
    return;
  }
  const grid = gridGroups(dock);
  const anchor = grid.find(hasSession) ?? grid[0];
  dock.addPanel({
    ...base,
    initialWidth: 560,
    ...(anchor && { position: { referenceGroup: anchor, direction: 'right' } }),
  });
}

/** Moves every floating panel back into the grid, as tabs of its first group. */
export function dockFloating(api: DockviewApi) {
  const floating = api.groups.filter((g) => g.api.location.type === 'floating');
  if (!floating.length) return;
  const target = gridGroups(api)[0] ?? api.addGroup();
  for (const group of floating)
    for (const panel of [...group.panels]) panel.api.moveTo({ group: target, position: 'center' });
}

export function attachMain(api: DockviewApi, taskId: number) {
  mainApi.value = api;
  currentTask = taskId;
  floatUnlocked.value = read(floatKey(taskId)) === '1';
  if (!floatUnlocked.value) dockFloating(api);
  layoutVersion.value++;
}

export function detachMain(api: DockviewApi) {
  if (mainApi.value !== api) return;
  mainApi.value = null;
  currentTask = null;
  layoutVersion.value++;
}

export function saveLayout(api: DockviewApi, taskId: number) {
  const json = JSON.stringify(api.toJSON());
  write(workspaceKey(taskId), json);
  // The last arrangement anyone touched is the starting point for tasks without their own.
  write(TEMPLATE_KEY, json);
  layoutVersion.value++;
}

/** Restores the sizes dockview spreads evenly whenever a group is removed. */
export function keepSizesOnRemove(api: DockviewApi): { dispose(): void }[] {
  let before: GroupSize[] = [];
  return [
    api.onWillMutateLayout((e) => {
      if (e.kind === 'remove')
        before = gridGroups(api).map((g) => ({
          id: g.id,
          width: g.api.width,
          height: g.api.height,
        }));
    }),
    api.onDidMutateLayout((e) => {
      if (e.kind !== 'remove') return;
      for (const size of keptSizes(
        before,
        gridGroups(api).map((g) => g.id),
      )) {
        api.getGroup(size.id)?.api.setSize({ width: size.width, height: size.height });
      }
    }),
  ];
}

export function paneState(pane: ToolPane): PaneState {
  void layoutVersion.value;
  const panel = mainApi.value?.getPanel(pane);
  if (!panel) return 'closed';
  return panel.group.activePanel === panel ? 'front' : 'background';
}

export function togglePane(pane: ToolPane) {
  const api = mainApi.value;
  if (!api) return;
  const action = toggleAction(paneState(pane));
  if (action === 'open') addTool(api, pane);
  else if (action === 'activate') api.getPanel(pane)?.api.setActive();
  else api.getPanel(pane)?.api.close();
}

/** Drops the current task's layout and the template; the remount builds the default. */
export function resetLayout() {
  if (currentTask === null) return;
  remove(workspaceKey(currentTask));
  remove(TEMPLATE_KEY);
  layoutEpoch.value++;
}

export function setFloatUnlocked(unlocked: boolean) {
  if (currentTask === null) return;
  floatUnlocked.value = unlocked;
  if (unlocked) write(floatKey(currentTask), '1');
  else {
    remove(floatKey(currentTask));
    if (mainApi.value) dockFloating(mainApi.value);
  }
}

export function pruneLayouts(taskIds: number[]) {
  [...staleKeys(keys(), taskIds), ...LEGACY_KEYS].forEach(remove);
}
