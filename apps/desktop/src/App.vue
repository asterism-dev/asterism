<script setup lang="ts">
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { api, errorMessage } from './api';
import MainDock from './components/MainDock.vue';
import PaneHandle from './components/PaneHandle.vue';
import ProjectPage from './components/ProjectPage.vue';
import Sidebar from './components/Sidebar.vue';
import TopBar from './components/TopBar.vue';
import { layoutEpoch, pruneLayouts, togglePane } from './dock/main';
import { SIDEBAR, clampWidth } from './dock/model';
import { sidebar } from './dock/sidebar';
import AddProjectDialog from './components/AddProjectDialog.vue';
import ContextMenu from './components/ContextMenu.vue';
import NewTaskDialog from './components/NewTaskDialog.vue';
import QuitDialog from './components/QuitDialog.vue';
import SettingsView from './components/SettingsView.vue';
import { activeTab, applyEvent, isConnected, nextWaiting, refresh, refreshPluginUpdates, selectSession, state, toast } from './store';
import { appShortcut } from './shortcuts';
import { leaveSettings } from './settingsGuard';
import type { NodeEvent, NodeStatus, Session } from './types';

const unlisteners: UnlistenFn[] = [];
const selectedTask = computed(() => state.tasks.find((t) => t.id === state.selectedTaskId) ?? null);
const quitRunning = ref<number | null>(null);
const columns = computed(() => (sidebar.open ? `${sidebar.width}px 0 minmax(0, 1fr)` : 'minmax(0, 1fr)'));

async function notify(session: Session) {
  const viewing = state.selectedTaskId === session.task_id && activeTab(state, session.task_id) === session.id;
  if (viewing && document.hasFocus()) return;
  const task = state.tasks.find((t) => t.id === session.task_id);
  const granted = (await isPermissionGranted()) || (await requestPermission()) === 'granted';
  if (granted) sendNotification({ title: 'asterism', body: `${task?.title ?? 'A session'} is waiting for you` });
}

function onStatus(status: NodeStatus) {
  const wasConnected = isConnected(state.node);
  state.node = status;
  if (!wasConnected && isConnected(status)) {
    // Archived tasks keep their layout for a later restore, so only deleted tasks are pruned.
    refresh().then(() => api.allTasks()).then((all) => pruneLayouts(all.map((t) => t.id))).catch((e) => toast(errorMessage(e)));
  }
}

function onKey(e: KeyboardEvent) {
  const key = appShortcut(e);
  if (key === 'n') {
    e.preventDefault();
    const projectId = selectedTask.value?.project_id ?? state.projects[0]?.id;
    if (projectId !== undefined) state.newTaskFor = projectId;
  } else if (key === 'left' || key === 'right') {
    e.preventDefault();
    if (key === 'left') sidebar.open = !sidebar.open;
    else togglePane('diff');
  } else if (key === 'j') {
    e.preventDefault();
    const session = nextWaiting(state);
    if (session) leaveSettings().then((left) => left && selectSession(state, session)).catch(() => {});
  }
}

function onQuitRequested() {
  const running = state.sessions.filter((s) => s.status !== 'exited').length;
  if (running === 0) api.quit(true).catch((e) => toast(errorMessage(e)));
  else quitRunning.value = running;
}

function restart() {
  api.restartDaemon().catch((e) => toast(errorMessage(e)));
}

onMounted(async () => {
  unlisteners.push(await listen<NodeStatus>('node-status', (e) => onStatus(e.payload)));
  unlisteners.push(
    await listen<NodeEvent>('node-event', (e) => {
      const event = e.payload;
      const restored = event.method === 'task.changed' && !event.params.archived && !state.tasks.some((t) => t.id === event.params.id);
      const waiting = applyEvent(state, event);
      if (event.method === 'plugins.changed' || event.method === 'stores.changed') refreshPluginUpdates().catch(() => {});
      if (event.method === 'plugins.changed') void api.agents().then((agents) => (state.agents = agents));
      // Archiving dropped the task's sessions from the store; a restore needs them back.
      if (restored) refresh().catch(() => {});
      if (waiting) notify(waiting).catch(() => {});
    }),
  );
  unlisteners.push(await listen('quit-requested', onQuitRequested));
  window.addEventListener('keydown', onKey);
  onStatus(await api.nodeStatus());
});

onUnmounted(() => {
  unlisteners.forEach((unlisten) => unlisten());
  window.removeEventListener('keydown', onKey);
});
</script>

<template>
  <div class="app" @click="state.menu = null">
    <div v-if="state.node.state === 'update_available'" class="banner">
      <template v-if="state.node.hello.daemon_version === state.node.bundled_version">
        Daemon is from another build ({{ state.node.hello.daemon_build || 'unknown' }} → {{ state.node.bundled_build }}).
      </template>
      <template v-else>Daemon update ready ({{ state.node.hello.daemon_version }} → {{ state.node.bundled_version }}).</template>
      <button @click="restart">Restart daemon</button>
    </div>
    <div v-else-if="state.node.state === 'incompatible'" class="banner error">
      {{ state.node.message }} <button @click="restart">Restart daemon</button>
    </div>
    <div class="workbench" :style="{ gridTemplateColumns: columns }">
      <template v-if="sidebar.open">
        <Sidebar />
        <PaneHandle side="left" :width="sidebar.width" @resize="sidebar.width = clampWidth($event)" @reset="sidebar.width = SIDEBAR.initial" />
      </template>
      <SettingsView v-if="state.settingsOpen" />
      <!-- v-show keeps terminals attached while Settings is open. -->
      <main v-show="!state.settingsOpen" class="main">
        <TopBar />
        <ProjectPage v-if="state.projectPage !== null" :project-id="state.projectPage" />
        <MainDock v-else-if="selectedTask" :key="`${selectedTask.id}-${layoutEpoch}`" :task="selectedTask" />
        <p v-else class="empty">Select a task, or create one with + Task.</p>
      </main>
    </div>
    <NewTaskDialog v-if="state.newTaskFor !== null" :project-id="state.newTaskFor" @close="state.newTaskFor = null" />
    <AddProjectDialog v-if="state.projectDialog" />
    <QuitDialog v-if="quitRunning !== null" :running="quitRunning" @close="quitRunning = null" />
    <ContextMenu />
    <div class="toasts">
      <div v-for="t in state.toasts" :key="t.id" class="toast">{{ t.message }}</div>
    </div>
  </div>
</template>
