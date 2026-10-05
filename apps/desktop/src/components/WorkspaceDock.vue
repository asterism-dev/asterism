<script setup lang="ts">
import { DockviewVue, getPanelData, themeLight, type DockviewApi, type DockviewReadyEvent, type VueComponent } from 'dockview-vue';
import { computed, onUnmounted, watch } from 'vue';
import { read, write } from '../dock/storage';
import { takePlacement } from '../dock/workspace';
import { parseWorkspace, placementPosition, reconcile, sessionIdOf, sessionPanelId, workspaceKey } from '../dock/workspaceModel';
import { newSessionMenu } from '../sessionActions';
import { activeTab, state, taskSessions } from '../store';
import type { Task } from '../types';
import GroupActions from './GroupActions.vue';
import SessionPane from './SessionPane.vue';
import SessionTab from './SessionTab.vue';

const props = defineProps<{ task: Task }>();
// dockview types panel components as prop-less; ours take its `params` prop.
const components = { session: SessionPane } as unknown as Record<string, VueComponent>;
const tabComponents = { session: SessionTab } as unknown as Record<string, VueComponent>;
const groupActions = GroupActions as unknown as VueComponent;
const sessionIds = computed(() => taskSessions(state, props.task.id).map((s) => s.id));
let dock: DockviewApi | null = null;
let lastGroup: string | null = null;
let disposables: { dispose(): void }[] = [];

function addPanel(api: DockviewApi, sessionId: number) {
  const position = placementPosition(takePlacement(sessionId), api.groups.map((g) => g.id), lastGroup);
  api.addPanel({ id: sessionPanelId(sessionId), component: 'session', tabComponent: 'session', params: { sessionId }, ...(position && { position }) });
}

function sync() {
  const api = dock;
  if (!api) return;
  const { remove, add } = reconcile(api.panels.map((p) => p.id), sessionIds.value);
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

function onReady(e: DockviewReadyEvent) {
  const api = e.api;
  const saved = parseWorkspace(read(workspaceKey(props.task.id)));
  if (saved) {
    try {
      api.fromJSON(saved);
    } catch {
      api.clear();
    }
  }
  dock = api;
  lastGroup = api.activeGroup?.id ?? null;
  sync();
  showSelected();
  disposables = [
    api.onWillShowOverlay((ev) => { if (ev.getData()?.viewId !== api.id) ev.preventDefault(); }),
    api.onDidActiveGroupChange((group) => { if (group) lastGroup = group.id; }),
    api.onDidActivePanelChange((ev) => {
      const id = ev.panel ? sessionIdOf(ev.panel.id) : null;
      if (id !== null) state.selectedTab[props.task.id] = id;
    }),
    api.onDidLayoutChange(() => write(workspaceKey(props.task.id), JSON.stringify(api.toJSON()))),
  ];
}

// Nested dockviews share one drop-target registry; if the outer one sees our drags it takes them over and the drop is lost.
function keepInside(e: DragEvent) {
  if (dock && getPanelData()?.viewId === dock.id) e.stopPropagation();
}

watch(sessionIds, sync);
watch(() => state.selectedTab[props.task.id], showSelected);
onUnmounted(() => disposables.forEach((d) => d.dispose()));
</script>

<template>
  <div class="pane" @dragenter="keepInside" @dragover="keepInside" @dragleave="keepInside" @drop="keepInside">
    <DockviewVue
      class="dock"
      :theme="themeLight"
      :components="components"
      :tab-components="tabComponents"
      :left-header-actions-component="groupActions"
      :disable-floating-groups="true"
      @ready="onReady"
    />
    <div v-if="!sessionIds.length" class="workspace-empty">
      <button class="primary" @click="newSessionMenu($event, task.id)">+ New session</button>
      <p class="muted">Start an agent or a terminal.</p>
    </div>
  </div>
</template>

<style scoped>
.workspace-empty { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: 8px; }
</style>
