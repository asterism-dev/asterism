<script setup lang="ts">
import {
  DockviewVue,
  themeLight,
  type DockviewApi,
  type DockviewReadyEvent,
  type VueComponent,
} from 'dockview-vue';
import { Plus } from '@lucide/vue';
import { computed, onUnmounted, ref, watch } from 'vue';
import {
  addTool,
  attachMain,
  detachMain,
  floatUnlocked,
  keepSizesOnRemove,
  saveLayout,
  takePlacement,
} from '../dock/main';
import {
  TEMPLATE_KEY,
  parseWorkspace,
  placementPosition,
  reconcile,
  sessionIdOf,
  sessionPanelId,
  workspaceKey,
} from '../dock/model';
import { read } from '../dock/storage';
import { newSessionMenu } from '../sessionActions';
import { activeTab, state, taskSessions } from '../store';
import type { Task } from '../types';
import GroupActions from './GroupActions.vue';
import PaneTab from './PaneTab.vue';
import SessionPane from './SessionPane.vue';
import ActivityPane from './panes/ActivityPane.vue';
import DiffPane from './panes/DiffPane.vue';
import FilePane from './panes/FilePane.vue';
import PluginPane from './panes/PluginPane.vue';

const props = defineProps<{ task: Task }>();
// dockview types panel components as prop-less; ours take its `params` prop.
const components = {
  session: SessionPane,
  diff: DiffPane,
  activity: ActivityPane,
  file: FilePane,
  plugin: PluginPane,
} as unknown as Record<string, VueComponent>;
const tabComponents = { pane: PaneTab } as unknown as Record<string, VueComponent>;
const groupActions = GroupActions as unknown as VueComponent;
const sessionIds = computed(() => taskSessions(state, props.task.id).map((s) => s.id));
const panelCount = ref(0);
let dock: DockviewApi | null = null;
let lastGroup: string | null = null;
let disposables: { dispose(): void }[] = [];

function addPanel(api: DockviewApi, sessionId: number) {
  // Grid groups first, so the fallbacks prefer docked groups; floating ones still count as explicit or focused targets.
  const ordered = [...api.groups].sort(
    (a, b) => Number(a.api.location.type !== 'grid') - Number(b.api.location.type !== 'grid'),
  );
  const groups = ordered.map((g) => ({
    id: g.id,
    hasSession: g.api.location.type === 'grid' && g.panels.some((p) => sessionIdOf(p.id) !== null),
  }));
  const position = placementPosition(takePlacement(sessionId), groups, lastGroup);
  api.addPanel({
    id: sessionPanelId(sessionId),
    component: 'session',
    tabComponent: 'pane',
    params: { sessionId },
    ...(position && { position }),
  });
}

function sync() {
  const api = dock;
  if (!api) return;
  const { remove, add } = reconcile(
    api.panels.map((p) => p.id),
    sessionIds.value,
    props.task.id,
  );
  for (const id of remove) {
    const panel = api.getPanel(id);
    if (panel) api.removePanel(panel);
  }
  add.forEach((id) => addPanel(api, id));
}

function showSelected() {
  const id = activeTab(state, props.task.id);
  const panel = id === null ? undefined : dock?.getPanel(sessionPanelId(id));
  if (panel && panel.group.activePanel !== panel) panel.api.setActive();
}

function load(api: DockviewApi): boolean {
  const saved =
    parseWorkspace(read(workspaceKey(props.task.id))) ?? parseWorkspace(read(TEMPLATE_KEY));
  if (!saved) return false;
  try {
    api.fromJSON(saved);
    return true;
  } catch {
    api.clear();
    return false;
  }
}

function onReady(e: DockviewReadyEvent) {
  const api = e.api;
  const loaded = load(api);
  dock = api;
  lastGroup = api.activeGroup?.id ?? null;
  sync();
  if (!loaded) {
    addTool(api, 'diff');
    addTool(api, 'activity', { background: true });
  }
  showSelected();
  attachMain(api, props.task.id);
  panelCount.value = api.panels.length;
  disposables = [
    ...keepSizesOnRemove(api),
    api.onDidActiveGroupChange((group) => {
      if (group) lastGroup = group.id;
    }),
    api.onDidActivePanelChange((ev) => {
      const id = ev.panel ? sessionIdOf(ev.panel.id) : null;
      if (id !== null) state.selectedTab[props.task.id] = id;
    }),
    api.onDidLayoutChange(() => {
      panelCount.value = api.panels.length;
      saveLayout(api, props.task.id);
    }),
  ];
  saveLayout(api, props.task.id);
}

watch(sessionIds, sync);
watch(() => state.selectedTab[props.task.id], showSelected);
onUnmounted(() => {
  disposables.forEach((d) => d.dispose());
  if (dock) detachMain(dock);
});
</script>

<template>
  <div class="main-dock">
    <DockviewVue
      class="dock"
      :theme="themeLight"
      :components="components"
      :tab-components="tabComponents"
      :left-header-actions-component="groupActions"
      :disable-floating-groups="!floatUnlocked"
      floating-group-drag-handle="tabbar"
      @ready="onReady"
    />
    <div v-if="!panelCount" class="workspace-empty">
      <button class="primary" @click="newSessionMenu($event, task.id)"><Plus />New session</button>
      <p class="muted">Start an agent or a terminal.</p>
    </div>
  </div>
</template>

<style scoped>
.main-dock {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.workspace-empty {
  position: absolute;
  inset: 0;
  display: grid;
  place-content: center;
  justify-items: center;
  gap: 8px;
}
</style>
