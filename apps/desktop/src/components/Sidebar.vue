<script setup lang="ts">
import { ask } from '@tauri-apps/plugin-dialog';
import { openUrl, revealItemInDir } from '@tauri-apps/plugin-opener';
import { ArrowDownUp, ChevronDown, ChevronRight, Plus, Settings, SquarePlus } from '@lucide/vue';
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import {
  isConnected, nextWaiting, selectSession, showMenu, state, taskStatus, toast,
  waitingSessions,
} from '../store';
import {
  filterTasks, loadCollapsed, loadSortMode, relativeTime, saveCollapsed, saveSortMode, sortProjects, sortTasks, type SortMode,
} from '../projects';
import { leaveSettings } from '../settingsGuard';
import { startSession } from '../sessionActions';
import { archiveTask, deleteTask } from '../taskActions';
import StatusIndicator from './StatusIndicator.vue';
import type { Project, Task } from '../types';

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
const sortMode = ref<SortMode>(loadSortMode());
const query = ref('');
const searching = computed(() => query.value.trim() !== '');
const visible = computed(() =>
  sortProjects(state.projects, state.tasks, sortMode.value).flatMap((project) => {
    const tasks = filterTasks(project, state.tasks, query.value);
    return tasks ? [{ project, tasks: sortTasks(tasks, sortMode.value) }] : [];
  }),
);
const now = ref(Date.now() / 1000);
let clock: ReturnType<typeof setInterval> | undefined;
const formatDate = (seconds: number) => new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
const report = (e: unknown) => toast(errorMessage(e));

function openIssue(url: string) {
  openUrl(url).catch(report);
}

onMounted(() => {
  Object.assign(state.collapsed, loadCollapsed());
  clock = setInterval(() => (now.value = Date.now() / 1000), 60_000);
});
onUnmounted(() => clearInterval(clock));
watch(sortMode, saveSortMode);

const SORT_LABELS: Record<SortMode, string> = { alphabetical: 'Alphabetical', activity: 'Activity', added: 'Last added' };

function sortMenu(e: MouseEvent) {
  showMenu(e, (Object.keys(SORT_LABELS) as SortMode[]).map((mode) => ({
    label: SORT_LABELS[mode],
    checked: sortMode.value === mode,
    action: () => (sortMode.value = mode),
  })));
}
watch(() => state.collapsed, saveCollapsed, { deep: true });

function toggle(p: Project) {
  if (state.collapsed[p.id]) delete state.collapsed[p.id];
  else state.collapsed[p.id] = true;
}

function openTask(t: Task) {
  leaveSettings().then((left) => {
    if (!left) return;
    state.projectPage = null;
    state.selectedTaskId = t.id;
  }).catch(report);
}

function openProject(p: Project) {
  leaveSettings().then((left) => { if (left) state.projectPage = p.id; }).catch(report);
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

function nodeMenu(e: MouseEvent) {
  showMenu(e, [
    { label: 'Settings…', action: () => (state.settingsOpen = true) },
    { label: 'Restart daemon', danger: true, action: restartDaemon },
  ]);
}

function projectMenu(e: MouseEvent, p: Project) {
  showMenu(e, [
    { label: 'Open project page', action: () => openProject(p) },
    { label: 'New task…', action: () => (state.newTaskFor = p.id) },
    { label: 'Reveal in file manager', action: () => revealItemInDir(p.path).catch(report) },
    { label: 'Remove project', danger: true, action: () => removeProject(p) },
  ]);
}

function taskMenu(e: MouseEvent, t: Task) {
  const agents = state.agents.filter((a) => a.available);
  showMenu(e, [
    ...agents.map((a) => ({ label: `New ${a.display_name || a.name} session`, action: () => startSession(t.id, { type: 'agent', name: a.name }) })),
    { label: 'New shell', action: () => startSession(t.id, { type: 'shell' }) },
    { label: 'Reveal worktree', action: () => revealItemInDir(t.worktree_path).catch(report) },
    { label: 'Archive task', action: () => archiveTask(t) },
    { label: 'Delete task', danger: true, action: () => deleteTask(t) },
  ]);
}
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar-scroll">
      <div class="row node-row" :class="{ offline: !connected }" @contextmenu="nodeMenu">
        <span class="name">{{ nodeName }}</span>
        <span v-if="!connected" class="muted offline-label" :title="offlineLabel">{{ offlineLabel }}</span>
        <button v-if="waiting.length" class="badge" @click="jump">{{ waiting.length }} waiting</button>
        <button class="add" :title="`Sort: ${SORT_LABELS[sortMode]}`" aria-label="Sort projects and tasks" @click.stop="sortMenu"><ArrowDownUp /></button>
        <button class="add" title="Add project" aria-label="Add project" @click="state.projectDialog = 'folder'"><SquarePlus /></button>
      </div>
      <input
        id="sidebar-search"
        v-model="query"
        class="search"
        type="search"
        placeholder="Search projects and tasks"
        aria-label="Search projects and tasks"
        @keydown.esc="query = ''"
      />
      <div v-for="{ project: p, tasks } in visible" :key="p.id" class="project" :class="{ offline: !connected }">
        <div class="row project-row" @contextmenu="projectMenu($event, p)">
          <button
            type="button"
            class="disclosure"
            :aria-expanded="!state.collapsed[p.id]"
            :aria-label="state.collapsed[p.id] ? `Expand ${p.name}` : `Collapse ${p.name}`"
            @click="toggle(p)"
          ><ChevronRight v-if="state.collapsed[p.id] && !searching" /><ChevronDown v-else /></button>
          <span class="name" :class="{ current: state.projectPage === p.id }" @click="openProject(p)">{{ p.name }}</span>
          <button class="hover-action" title="New task" @click="state.newTaskFor = p.id"><Plus />Task</button>
        </div>
        <template v-if="searching || !state.collapsed[p.id]">
          <div
            v-for="t in tasks"
            :key="t.id"
            class="row task-row"
            :class="{ selected: state.selectedTaskId === t.id }"
            @click="openTask(t)"
            @contextmenu="taskMenu($event, t)"
          >
            <span class="name">{{ t.title }}</span>
            <a v-if="t.issue" class="issue-key muted" :href="t.issue.url" :title="t.issue.url"
              @click.prevent.stop="openIssue(t.issue.url)">{{ t.issue.key }}</a>
            <span class="age muted" :title="`Created ${formatDate(t.created_at)} · Last activity ${formatDate(t.last_activity_at)}`">
              {{ relativeTime(t.last_activity_at, now) }}
            </span>
            <button class="hover-action" title="Archive task" @click.stop="archiveTask(t)">Archive</button>
            <StatusIndicator :status="taskStatus(state, t.id)" />
          </div>
        </template>
      </div>
      <p v-if="searching && !visible.length" class="hint muted">No matches</p>
      <p v-if="connected && !state.projects.length" class="hint">Click + to add, clone or create a project.</p>
    </div>
    <button class="settings-button" :class="{ active: state.settingsOpen }" @click="state.settingsOpen = true"><Settings />Settings<span v-if="state.pluginUpdates" class="badge update-badge" :title="`${state.pluginUpdates} plugin update(s)`">{{ state.pluginUpdates }}</span></button>
  </aside>
</template>

<style scoped>
.sidebar { background: var(--panel); border-right: 1px solid var(--border); display: flex; flex-direction: column; min-height: 0; }
.sidebar-scroll { flex: 1; overflow-y: auto; padding: 8px 6px; }
.row { display: flex; align-items: center; gap: 8px; padding: 4px 6px; border-radius: 6px; min-height: 28px; }
.issue-key { flex: none; font-size: 12px; text-decoration: none; }
.row .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.node-row { font-weight: 600; }
.project-row { margin-top: 8px; font-weight: 500; }
.disclosure { border: 0; padding: 0 2px; background: transparent; width: 16px; color: var(--muted); }
.project-row .name { cursor: pointer; }
.project-row .name.current { font-weight: 600; }
.task-row { padding-left: 30px; cursor: default; }
.task-row:hover, .project-row:hover { background: var(--select); }
.task-row.selected { background: var(--select); }
.hover-action { visibility: hidden; padding: 0 6px; font-size: 12px; }
.row:hover .hover-action { visibility: visible; }
.badge { background: var(--waiting); color: #1d1f27; border: 0; padding: 0 8px; border-radius: 10px; font-size: 12px; }
.update-badge { margin-left: 6px; }
.add { padding: 0 7px; }
.age { flex: none; font-size: 12px; font-variant-numeric: tabular-nums; }
.offline-label { flex-shrink: 1; min-width: 0; max-width: 50%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 400; }
.offline { opacity: 0.55; }
.hint { padding: 0 8px; }
.search { width: 100%; margin: 6px 0 2px; box-sizing: border-box; }
.settings-button { flex: none; margin: 0; padding: 8px 12px; width: 100%; text-align: left; border: 0; border-top: 1px solid var(--border); border-radius: 0; }
.settings-button.active { background: var(--select); }
</style>
