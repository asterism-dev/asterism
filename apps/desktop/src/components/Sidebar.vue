<script setup lang="ts">
import { ask, open } from '@tauri-apps/plugin-dialog';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { computed, onMounted, watch } from 'vue';
import { api, errorMessage, RpcError } from '../api';
import {
  addSession, isConnected, nextWaiting, nodeAggregateStatus, projectStatus, selectSession, showMenu, state, taskStatus, toast,
  waitingSessions,
} from '../store';
import { loadCollapsed, saveCollapsed } from '../projects';
import { leaveSettings } from '../settingsGuard';
import type { Project, SessionKind, Task } from '../types';

const connected = computed(() => isConnected(state.node));
const nodeName = computed(() => ('hello' in state.node ? state.node.hello.hostname : 'This computer'));
const waiting = computed(() => waitingSessions(state));
const offlineLabel = computed(() => {
  switch (state.node.state) {
    case 'connecting': return 'starting…';
    case 'disconnected': return state.node.reason;
    case 'incompatible': return 'incompatible daemon';
    default: return '';
  }
});
const tasksOf = (p: Project) => state.tasks.filter((t) => t.project_id === p.id);
const report = (e: unknown) => toast(errorMessage(e));

onMounted(() => Object.assign(state.collapsed, loadCollapsed()));
watch(() => state.collapsed, saveCollapsed, { deep: true });

function toggle(p: Project) {
  if (state.collapsed[p.id]) delete state.collapsed[p.id];
  else state.collapsed[p.id] = true;
}

async function addFolder() {
  const path = await open({ directory: true, multiple: false });
  if (typeof path === 'string') api.addProject(path).catch(report);
}

async function archive(task: Task) {
  try {
    await api.archiveTask(task.id, false);
  } catch (e) {
    if (!(e instanceof RpcError && e.kind === 'dirty_worktree')) return report(e);
    const discard = await ask(
      `"${task.title}" has uncommitted changes. Archive it anyway and discard them? The branch ${task.branch} is kept.`,
      { title: 'Archive task', kind: 'warning' },
    );
    if (discard) api.archiveTask(task.id, true).catch(report);
  }
}

function start(task: Task, kind: SessionKind) {
  api.startSession(task.id, kind).then((s) => addSession(state, s)).catch(report);
}

function openTask(t: Task) {
  leaveSettings().then((left) => { if (left) state.selectedTaskId = t.id; }).catch(report);
}

function jump() {
  const session = nextWaiting(state);
  if (session) leaveSettings().then((left) => left && selectSession(state, session)).catch(report);
}

async function restartDaemon() {
  const confirmed = await ask('Restart the asterism daemon? Running sessions may stop or be resumed.', {
    title: 'Restart daemon', kind: 'warning',
  });
  if (confirmed) api.restartDaemon().catch(report);
}

async function removeProject(p: Project) {
  const confirmed = await ask(`Remove "${p.name}" from asterism?`, { title: 'Remove project', kind: 'warning' });
  if (confirmed) api.removeProject(p.id).catch(report);
}

function addProjectMenu(e: MouseEvent) {
  showMenu(e, [
    { label: 'Add folder…', action: addFolder },
    { label: 'Clone repository…', action: () => (state.projectDialog = 'clone') },
    { label: 'New repository…', action: () => (state.projectDialog = 'create') },
  ]);
}

function nodeMenu(e: MouseEvent) {
  showMenu(e, [
    { label: 'Settings…', action: () => (state.settingsOpen = true) },
    { label: 'Add folder…', action: addFolder },
    { label: 'Clone repository…', action: () => (state.projectDialog = 'clone') },
    { label: 'New repository…', action: () => (state.projectDialog = 'create') },
    { label: 'Restart daemon', danger: true, action: restartDaemon },
  ]);
}

function projectMenu(e: MouseEvent, p: Project) {
  showMenu(e, [
    { label: 'New task…', action: () => (state.newTaskFor = p.id) },
    { label: 'Reveal in file manager', action: () => revealItemInDir(p.path).catch(report) },
    { label: 'Remove project', danger: true, action: () => removeProject(p) },
  ]);
}

function taskMenu(e: MouseEvent, t: Task) {
  const agents = 'hello' in state.node ? state.node.hello.agents.filter((a) => a.available) : [];
  showMenu(e, [
    ...agents.map((a) => ({ label: `New ${a.name} session`, action: () => start(t, { type: 'agent', name: a.name }) })),
    { label: 'New shell', action: () => start(t, { type: 'shell' }) },
    { label: 'Reveal worktree', action: () => revealItemInDir(t.worktree_path).catch(report) },
    { label: 'Archive task', danger: true, action: () => archive(t) },
  ]);
}
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar-scroll">
      <div class="row node-row" :class="{ offline: !connected }" @contextmenu="nodeMenu">
        <span class="dot" :class="connected ? (nodeAggregateStatus(state) ?? 'idle') : ''"></span>
        <span class="name">{{ nodeName }}</span>
        <span v-if="!connected" class="muted offline-label" :title="offlineLabel">{{ offlineLabel }}</span>
        <button v-if="waiting.length" class="badge" @click="jump">{{ waiting.length }} waiting</button>
        <button class="add" title="Add project" @click="addProjectMenu">+</button>
      </div>
      <div v-for="p in state.projects" :key="p.id" class="project" :class="{ offline: !connected }">
        <div class="row project-row" @contextmenu="projectMenu($event, p)">
          <button
            class="disclosure"
            :aria-expanded="!state.collapsed[p.id]"
            :aria-label="state.collapsed[p.id] ? `Expand ${p.name}` : `Collapse ${p.name}`"
            @click="toggle(p)"
          >{{ state.collapsed[p.id] ? '▸' : '▾' }}</button>
          <span class="dot" :class="projectStatus(state, p.id) ?? ''"></span>
          <span class="name" @click="toggle(p)">{{ p.name }}</span>
          <button class="hover-action" title="New task" @click="state.newTaskFor = p.id">+ Task</button>
        </div>
        <template v-if="!state.collapsed[p.id]">
          <div
            v-for="t in tasksOf(p)"
            :key="t.id"
            class="row task-row"
            :class="{ selected: state.selectedTaskId === t.id }"
            @click="openTask(t)"
            @contextmenu="taskMenu($event, t)"
          >
            <span class="dot" :class="taskStatus(state, t.id) ?? ''"></span>
            <span class="name">{{ t.title }}</span>
            <button class="hover-action" title="Archive task" @click.stop="archive(t)">Archive</button>
          </div>
        </template>
      </div>
      <p v-if="connected && !state.projects.length" class="hint">Drop a git repository onto the window, or click + to add, clone or create a project.</p>
    </div>
    <button class="settings-button" :class="{ active: state.settingsOpen }" @click="state.settingsOpen = true">⚙ Settings</button>
  </aside>
</template>

<style scoped>
.sidebar { background: var(--panel); border-right: 1px solid var(--border); display: flex; flex-direction: column; min-height: 0; }
.sidebar-scroll { flex: 1; overflow-y: auto; padding: 8px 6px; }
.row { display: flex; align-items: center; gap: 8px; padding: 4px 6px; border-radius: 6px; min-height: 28px; }
.row .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.node-row { font-weight: 600; }
.project-row { margin-top: 8px; font-weight: 500; }
.disclosure { border: 0; padding: 0 2px; background: transparent; width: 16px; color: var(--muted); }
.project-row .name { cursor: default; }
.task-row { padding-left: 22px; cursor: default; }
.task-row:hover, .project-row:hover { background: var(--select); }
.task-row.selected { background: var(--select); }
.hover-action { visibility: hidden; padding: 0 6px; font-size: 12px; }
.row:hover .hover-action { visibility: visible; }
.badge { background: var(--waiting); color: #1d1f27; border: 0; padding: 0 8px; border-radius: 10px; font-size: 12px; }
.add { padding: 0 7px; }
.offline-label { flex-shrink: 1; min-width: 0; max-width: 50%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 400; }
.offline { opacity: 0.55; }
.hint { padding: 0 8px; }
.settings-button { flex: none; margin: 0; padding: 8px 12px; width: 100%; text-align: left; border: 0; border-top: 1px solid var(--border); border-radius: 0; }
.settings-button.active { background: var(--select); }
</style>
