<script setup lang="ts">
import { ask, open } from '@tauri-apps/plugin-dialog';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { computed } from 'vue';
import { api, errorMessage, RpcError } from '../api';
import {
  addSession, isConnected, nextWaiting, projectStatus, selectSession, showMenu, state, taskStatus, toast,
  waitingSessions,
} from '../store';
import type { Project, SessionKind, Task } from '../types';

const connected = computed(() => isConnected(state.node));
const nodeName = computed(() => ('hello' in state.node ? state.node.hello.hostname : 'This computer'));
const waiting = computed(() => waitingSessions(state));
const tasksOf = (p: Project) => state.tasks.filter((t) => t.project_id === p.id);
const report = (e: unknown) => toast(errorMessage(e));

async function addProject() {
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

function jump() {
  const session = nextWaiting(state);
  if (session) selectSession(state, session);
}

function nodeMenu(e: MouseEvent) {
  showMenu(e, [
    { label: 'Add project…', action: addProject },
    { label: 'Restart daemon', danger: true, action: () => api.restartDaemon().catch(report) },
  ]);
}

function projectMenu(e: MouseEvent, p: Project) {
  showMenu(e, [
    { label: 'New task…', action: () => (state.newTaskFor = p.id) },
    { label: 'Reveal in file manager', action: () => revealItemInDir(p.path).catch(report) },
    { label: 'Remove project', danger: true, action: () => api.removeProject(p.id).catch(report) },
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
    <div class="row node-row" :class="{ offline: !connected }" @contextmenu="nodeMenu">
      <span class="dot" :class="connected ? 'idle' : ''"></span>
      <span class="name">{{ nodeName }}</span>
      <span v-if="!connected" class="muted">reconnecting…</span>
      <button v-if="waiting.length" class="badge" @click="jump">{{ waiting.length }} waiting</button>
      <button class="add" title="Add project" @click="addProject">+</button>
    </div>
    <div v-for="p in state.projects" :key="p.id" class="project" :class="{ offline: !connected }">
      <div class="row project-row" @contextmenu="projectMenu($event, p)">
        <span class="dot" :class="projectStatus(state, p.id) ?? ''"></span>
        <span class="name">{{ p.name }}</span>
        <button class="hover-action" title="New task" @click="state.newTaskFor = p.id">+ Task</button>
      </div>
      <div
        v-for="t in tasksOf(p)"
        :key="t.id"
        class="row task-row"
        :class="{ selected: state.selectedTaskId === t.id }"
        @click="state.selectedTaskId = t.id"
        @contextmenu="taskMenu($event, t)"
      >
        <span class="dot" :class="taskStatus(state, t.id) ?? ''"></span>
        <span class="name">{{ t.title }}</span>
        <button class="hover-action" title="Archive task" @click.stop="archive(t)">Archive</button>
      </div>
    </div>
    <p v-if="connected && !state.projects.length" class="hint">Drop a git repository onto the window, or click + to add a project.</p>
  </aside>
</template>

<style scoped>
.sidebar { background: var(--panel); border-right: 1px solid var(--border); overflow-y: auto; padding: 8px 6px; }
.row { display: flex; align-items: center; gap: 8px; padding: 4px 6px; border-radius: 6px; min-height: 28px; }
.row .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.node-row { font-weight: 600; }
.project-row { margin-top: 8px; font-weight: 500; }
.task-row { padding-left: 22px; cursor: default; }
.task-row:hover, .project-row:hover { background: var(--select); }
.task-row.selected { background: var(--select); }
.hover-action { visibility: hidden; padding: 0 6px; font-size: 12px; }
.row:hover .hover-action { visibility: visible; }
.badge { background: var(--waiting); color: #1d1f27; border: 0; padding: 0 8px; border-radius: 10px; font-size: 12px; }
.add { padding: 0 7px; }
.offline { opacity: 0.55; }
.hint { padding: 0 8px; }
</style>
