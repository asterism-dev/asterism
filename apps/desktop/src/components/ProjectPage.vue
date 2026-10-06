<script setup lang="ts">
import { openUrl, revealItemInDir } from '@tauri-apps/plugin-opener';
import { ask } from '@tauri-apps/plugin-dialog';
import { computed, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import PrBadge from './PrBadge.vue';
import { prSummary } from '../prBadge';
import { filterTasks, formatSize, taskState, worktreeActions, type TaskFilter } from '../projectPage';
import { relativeTime } from '../projects';
import { applyPrList, state, toast } from '../store';
import { archiveTask, deleteTask, restoreTask } from '../taskActions';
import type { Task, Worktree } from '../types';

const props = defineProps<{ projectId: number }>();
const report = (e: unknown) => toast(errorMessage(e));

function openIssue(url: string) { openUrl(url).catch(report); }
const project = computed(() => state.projects.find((p) => p.id === props.projectId) ?? null);
const tab = ref<'tasks' | 'worktrees'>('tasks');
const filter = ref<TaskFilter>('all');
const tasks = ref<Task[]>([]);
const worktrees = ref<Worktree[]>([]);
const sizes = ref<Record<string, number>>({});
const now = ref(Date.now() / 1000);
const shown = computed(() => filterTasks(tasks.value, filter.value));
const taskTitle = (id: number | null) => tasks.value.find((t) => t.id === id)?.title ?? '—';

async function loadTasks() {
  tasks.value = await api.projectTasks(props.projectId).catch((e) => (report(e), []));
  now.value = Date.now() / 1000;
}

async function loadWorktrees() {
  worktrees.value = await api.worktrees(props.projectId).catch((e) => (report(e), []));
  const projectId = props.projectId;
  const measured = await api.worktreeSizes(projectId).catch(() => []);
  if (projectId === props.projectId) sizes.value = Object.fromEntries(measured.map((s) => [s.path, s.bytes]));
}

function openTask(id: number | null) {
  if (id === null) return;
  state.projectPage = null;
  state.selectedTaskId = id;
}

async function removeWorktree(w: Worktree) {
  const confirmed = await ask(`Remove the worktree at ${w.path}? Uncommitted changes there are lost.`, { title: 'Remove worktree', kind: 'warning' });
  if (confirmed) api.removeWorktree(props.projectId, w.path).then(loadWorktrees).catch(report);
}

watch(() => [props.projectId, state.tasksVersion], loadTasks, { immediate: true });
watch(() => props.projectId, (id) => {
  api.refreshPrs(id).then((l) => applyPrList(state, l, id)).catch(report);
}, { immediate: true });
watch(tab, (t) => { if (t === 'worktrees') loadWorktrees(); });
watch(() => props.projectId, () => { sizes.value = {}; if (tab.value === 'worktrees') loadWorktrees(); });
</script>

<template>
  <section class="project-page">
    <nav class="tabs" role="tablist">
      <button role="tab" :aria-selected="tab === 'tasks'" :class="{ active: tab === 'tasks' }" @click="tab = 'tasks'">Tasks</button>
      <button role="tab" :aria-selected="tab === 'worktrees'" :class="{ active: tab === 'worktrees' }" @click="tab = 'worktrees'">Worktrees</button>
    </nav>
    <div v-if="tab === 'tasks'" class="body">
      <div class="segmented" role="group" aria-label="Filter">
        <button v-for="f in (['all', 'active', 'archived'] as const)" :key="f" :aria-pressed="filter === f" :class="{ active: filter === f }" @click="filter = f">{{ f }}</button>
      </div>
      <p v-if="state.prErrors[projectId]" class="error">PR status: {{ state.prErrors[projectId] }}</p>
      <table>
        <thead><tr><th>Task</th><th>Branch</th><th>PR</th><th>Created</th><th>Activity</th><th>State</th><th></th></tr></thead>
        <tbody>
          <tr v-for="t in shown" :key="t.id">
            <td>{{ t.title }} <a v-if="t.issue" class="muted" :href="t.issue.url" @click.prevent="openIssue(t.issue.url)">{{ t.issue.key }}</a></td>
            <td class="mono">{{ t.branch }}</td>
            <td><template v-if="state.prs[t.id]"><PrBadge :pr="state.prs[t.id]!" /> <span class="muted">{{ prSummary(state.prs[t.id]!) }}</span></template></td>
            <td class="muted">{{ relativeTime(t.created_at, now) }}</td>
            <td class="muted">{{ relativeTime(t.last_activity_at, now) }}</td>
            <td>{{ taskState(t, state.sessions) }}</td>
            <td class="actions">
              <button v-if="!t.archived" @click="openTask(t.id)">Open</button>
              <button v-if="!t.archived" @click="archiveTask(t)">Archive</button>
              <button v-else @click="restoreTask(t)">Restore</button>
              <button class="danger" @click="deleteTask(t)">Delete</button>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-if="!shown.length" class="muted">No tasks.</p>
    </div>
    <div v-else class="body">
      <div v-if="worktrees.some((w) => w.prunable)" class="toolbar">
        <button @click="api.pruneWorktrees(projectId).then(loadWorktrees).catch(report)">Prune missing worktrees</button>
      </div>
      <table>
        <thead><tr><th>Path</th><th>Branch</th><th>Base</th><th class="num">Size</th><th>Task</th><th></th></tr></thead>
        <tbody>
          <tr v-for="w in worktrees" :key="w.path">
            <td class="mono path" :title="w.path">
              {{ w.path }}
              <span v-if="w.is_main" class="flag">main</span>
              <span v-if="w.locked" class="flag">locked</span>
              <span v-if="w.prunable" class="flag">missing</span>
            </td>
            <td class="mono">{{ w.branch ?? `detached @ ${w.head.slice(0, 7)}` }}</td>
            <td class="mono muted">{{ w.base_branch ?? '—' }}</td>
            <td class="num">{{ formatSize(sizes[w.path]) }}</td>
            <td>{{ w.task_id === null ? '—' : taskTitle(w.task_id) }}</td>
            <td class="actions">
              <button v-if="!w.prunable" @click="revealItemInDir(w.path).catch(report)">Reveal</button>
              <button v-if="worktreeActions(w).open" @click="openTask(w.task_id)">Open task</button>
              <button v-if="worktreeActions(w).remove" class="danger" @click="removeWorktree(w)">Remove</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-if="!project" class="muted">This project no longer exists.</p>
  </section>
</template>

<style scoped>
.project-page { display: flex; flex-direction: column; flex: 1; min-height: 0; }
.tabs { display: flex; gap: 4px; padding: 8px 14px 0; border-bottom: 1px solid var(--border); background: var(--panel); }
.tabs button { border: 0; border-radius: 6px 6px 0 0; }
.tabs button.active { background: var(--select); }
.body { overflow: auto; padding: 12px 14px; display: flex; flex-direction: column; gap: 10px; }
table { width: 100%; border-collapse: collapse; }
th { text-align: left; font-weight: 600; color: var(--muted); font-size: 12px; }
th, td { padding: 6px 8px; border-bottom: 1px solid var(--border); vertical-align: middle; }
.mono { font-family: ui-monospace, Menlo, monospace; font-size: 12px; }
.path { max-width: 360px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
.actions { white-space: nowrap; text-align: right; }
.actions button { margin-left: 4px; }
button.danger { color: var(--danger); }
.flag { margin-left: 6px; font-size: 11px; padding: 0 5px; border: 1px solid var(--border); border-radius: 8px; color: var(--muted); }
.segmented button { text-transform: capitalize; }
</style>
