<script setup lang="ts">
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';
import { computed, onMounted, onUnmounted } from 'vue';
import { api, errorMessage } from './api';
import CloneDialog from './components/CloneDialog.vue';
import ContextMenu from './components/ContextMenu.vue';
import CreateRepoDialog from './components/CreateRepoDialog.vue';
import NewTaskDialog from './components/NewTaskDialog.vue';
import SettingsView from './components/SettingsView.vue';
import Sidebar from './components/Sidebar.vue';
import TaskView from './components/TaskView.vue';
import { activeTab, applyEvent, isConnected, nextWaiting, refresh, selectSession, state, toast } from './store';
import { appShortcut } from './shortcuts';
import { leaveSettings } from './settingsGuard';
import type { NodeEvent, NodeStatus, Session } from './types';

const unlisteners: UnlistenFn[] = [];
const selectedTask = computed(() => state.tasks.find((t) => t.id === state.selectedTaskId) ?? null);

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
  if (!wasConnected && isConnected(status)) refresh().catch((e) => toast(errorMessage(e)));
}

function onKey(e: KeyboardEvent) {
  const key = appShortcut(e);
  if (key === 'n') {
    e.preventDefault();
    const projectId = selectedTask.value?.project_id ?? state.projects[0]?.id;
    if (projectId !== undefined) state.newTaskFor = projectId;
  } else if (key === 'j') {
    e.preventDefault();
    const session = nextWaiting(state);
    if (session) leaveSettings().then((left) => left && selectSession(state, session)).catch(() => {});
  }
}

function restart() {
  api.restartDaemon().catch((e) => toast(errorMessage(e)));
}

onMounted(async () => {
  unlisteners.push(await listen<NodeStatus>('node-status', (e) => onStatus(e.payload)));
  unlisteners.push(
    await listen<NodeEvent>('node-event', (e) => {
      const waiting = applyEvent(state, e.payload);
      if (waiting) notify(waiting).catch(() => {});
    }),
  );
  unlisteners.push(
    await getCurrentWebview().onDragDropEvent(async (e) => {
      if (e.payload.type !== 'drop') return;
      for (const path of e.payload.paths) {
        await api.addProject(path).catch((err) => toast(`${path}: ${errorMessage(err)}`));
      }
    }),
  );
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
    <Sidebar />
    <main class="main">
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
      <SettingsView v-if="state.settingsOpen" />
      <TaskView v-else-if="selectedTask" :key="selectedTask.id" :task="selectedTask" />
      <p v-else class="empty">Select a task, or create one with + Task.</p>
    </main>
    <NewTaskDialog v-if="state.newTaskFor !== null" :project-id="state.newTaskFor" @close="state.newTaskFor = null" />
    <CloneDialog v-if="state.projectDialog === 'clone'" @close="state.projectDialog = null" />
    <CreateRepoDialog v-if="state.projectDialog === 'create'" @close="state.projectDialog = null" />
    <ContextMenu />
    <div class="toasts">
      <div v-for="t in state.toasts" :key="t.id" class="toast">{{ t.message }}</div>
    </div>
  </div>
</template>
