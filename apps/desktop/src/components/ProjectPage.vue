<script setup lang="ts">
import {
  Button,
  Checkbox,
  Field,
  SegmentedControl,
  Select,
  Table,
  Tabs,
  type TableColumn,
} from '@asterism-dev/lumen';
import { openUrl, revealItemInDir } from '@tauri-apps/plugin-opener';
import { ask } from '@tauri-apps/plugin-dialog';
import { computed, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import BulkDeleteDialog from './BulkDeleteDialog.vue';
import PrBadge from './PrBadge.vue';
import { prSummary } from '../prBadge';
import { staleDefault } from '../baseBranch';
import {
  filterTasks,
  formatSize,
  taskState,
  worktreeActions,
  type TaskFilter,
} from '../projectPage';
import { relativeTime } from '../projects';
import { applyPrList, state, toast } from '../store';
import { archiveTask, archiveTasks, deleteTask, restoreTask, restoreTasks } from '../taskActions';
import type { ProjectBranches, Task, Worktree } from '../types';

const props = defineProps<{ projectId: number }>();
const report = (e: unknown) => toast(errorMessage(e));

function openIssue(url: string) {
  openUrl(url).catch(report);
}
const project = computed(() => state.projects.find((p) => p.id === props.projectId) ?? null);
const TABS = [
  { value: 'tasks', label: 'Tasks' },
  { value: 'worktrees', label: 'Worktrees' },
  { value: 'settings', label: 'Settings' },
];
const FILTERS: { value: TaskFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'active', label: 'Active' },
  { value: 'archived', label: 'Archived' },
];
const TASK_COLUMNS: TableColumn[] = [
  { key: 'pick', label: 'Select', class: 'pick' },
  { key: 'title', label: 'Task' },
  { key: 'branch', label: 'Branch', class: 'mono' },
  { key: 'pr', label: 'PR' },
  { key: 'created', label: 'Created', class: 'muted' },
  { key: 'activity', label: 'Activity', class: 'muted' },
  { key: 'state', label: 'State' },
  { key: 'actions', label: '', class: 'actions' },
];
const WORKTREE_COLUMNS: TableColumn[] = [
  { key: 'path', label: 'Path', class: 'mono path' },
  { key: 'branch', label: 'Branch', class: 'mono' },
  { key: 'base', label: 'Base', class: 'mono muted' },
  { key: 'size', label: 'Size', align: 'right', class: 'num' },
  { key: 'task', label: 'Task' },
  { key: 'actions', label: '', class: 'actions' },
];
const tab = ref('tasks');
const branches = ref<ProjectBranches | null>(null);
const filter = ref<TaskFilter>('all');
const tasks = ref<Task[]>([]);
const worktrees = ref<Worktree[]>([]);
const sizes = ref<Record<string, number>>({});
const now = ref(Date.now() / 1000);
const shown = computed(() => filterTasks(tasks.value, filter.value));
const picked = ref(new Set<number>());
const selected = computed(() => shown.value.filter((t) => picked.value.has(t.id)));
const allPicked = computed(
  () => shown.value.length > 0 && selected.value.length === shown.value.length,
);
const bulkDeleting = ref(false);
const taskTitle = (id: number | null) => tasks.value.find((t) => t.id === id)?.title ?? '—';

async function loadTasks() {
  tasks.value = await api.projectTasks(props.projectId).catch((e) => (report(e), []));
  now.value = Date.now() / 1000;
}

async function loadWorktrees() {
  worktrees.value = await api.worktrees(props.projectId).catch((e) => (report(e), []));
  const projectId = props.projectId;
  const measured = await api.worktreeSizes(projectId).catch(() => []);
  if (projectId === props.projectId)
    sizes.value = Object.fromEntries(measured.map((s) => [s.path, s.bytes]));
}

async function loadBranches() {
  const id = props.projectId;
  const result = await api.projectBranches(id).catch((e) => (report(e), null));
  if (id === props.projectId) branches.value = result;
}

function setDefaultBase(value: string) {
  api
    .updateProject(props.projectId, value || null)
    .then(loadBranches)
    .catch(report);
}

// A local copy so a failed move can put the select back.
const workspaceId = ref(0);
watch(
  () => project.value?.workspace_id,
  (id) => (workspaceId.value = id ?? 0),
  { immediate: true },
);

function moveToWorkspace(id: number) {
  api.moveProject(props.projectId, id).catch((e) => {
    workspaceId.value = project.value?.workspace_id ?? 0;
    report(e);
  });
}

function togglePick(id: number) {
  const next = new Set(picked.value);
  if (!next.delete(id)) next.add(id);
  picked.value = next;
}

function toggleAll() {
  picked.value = new Set(allPicked.value ? [] : shown.value.map((t) => t.id));
}

async function bulk(run: (tasks: Task[]) => Promise<void>, tasks: Task[]) {
  await run(tasks);
  picked.value = new Set();
}

function openTask(id: number | null) {
  if (id === null) return;
  state.projectPage = null;
  state.selectedTaskId = id;
}

async function removeWorktree(w: Worktree) {
  const confirmed = await ask(
    `Remove the worktree at ${w.path}? Uncommitted changes there are lost.`,
    { title: 'Remove worktree', kind: 'warning' },
  );
  if (confirmed) api.removeWorktree(props.projectId, w.path).then(loadWorktrees).catch(report);
}

watch(() => [props.projectId, state.tasksVersion], loadTasks, { immediate: true });
watch([filter, () => props.projectId], () => (picked.value = new Set()));
watch(
  () => props.projectId,
  (id) => {
    api
      .refreshPrs(id)
      .then((l) => applyPrList(state, l, id))
      .catch(report);
  },
  { immediate: true },
);
watch(tab, (t) => {
  if (t === 'worktrees') loadWorktrees();
  if (t === 'settings') loadBranches();
});
watch(
  () => props.projectId,
  () => {
    sizes.value = {};
    branches.value = null;
    if (tab.value === 'worktrees') loadWorktrees();
    if (tab.value === 'settings') loadBranches();
  },
);
</script>

<template>
  <section class="project-page">
    <Tabs v-model="tab" :tabs="TABS" class="tabs">
      <template #tasks>
        <div class="body">
          <SegmentedControl v-model="filter" :options="FILTERS" aria-label="Filter" />
          <div v-if="selected.length" class="toolbar bulk">
            <span>{{ selected.length }} selected</span>
            <Button
              v-if="selected.some((t) => !t.archived)"
              @click="
                bulk(
                  archiveTasks,
                  selected.filter((t) => !t.archived),
                )
              "
            >
              Archive
            </Button>
            <Button
              v-if="selected.some((t) => t.archived)"
              @click="
                bulk(
                  restoreTasks,
                  selected.filter((t) => t.archived),
                )
              "
            >
              Restore
            </Button>
            <Button variant="danger" @click="bulkDeleting = true">Delete</Button>
          </div>
          <p v-if="state.prErrors[projectId]" class="error">
            PR status: {{ state.prErrors[projectId] }}
          </p>
          <Table :columns="TASK_COLUMNS" :rows="shown" row-key="id" size="sm" hover="clickable">
            <template #header-pick>
              <Checkbox
                aria-label="Select all tasks"
                :model-value="allPicked || (selected.length > 0 && 'indeterminate')"
                @update:model-value="toggleAll"
              />
            </template>
            <template #cell-pick="{ row: t }">
              <Checkbox
                :aria-label="`Select ${t.title}`"
                :model-value="picked.has(t.id)"
                @update:model-value="togglePick(t.id)"
              />
            </template>
            <template #cell-title="{ row: t }">
              {{ t.title }}
              <a
                v-if="t.issue"
                class="muted"
                :href="t.issue.url"
                @click.prevent="openIssue(t.issue.url)"
                >{{ t.issue.key }}</a
              >
            </template>
            <template #cell-pr="{ row: t }">
              <template v-if="state.prs[t.id]"
                ><PrBadge :pr="state.prs[t.id]!" />
                <span class="muted">{{ prSummary(state.prs[t.id]!) }}</span></template
              >
            </template>
            <template #cell-created="{ row: t }">{{ relativeTime(t.created_at, now) }}</template>
            <template #cell-activity="{ row: t }">{{
              relativeTime(t.last_activity_at, now)
            }}</template>
            <template #cell-state="{ row: t }">{{ taskState(t, state.sessions) }}</template>
            <template #cell-actions="{ row: t }">
              <Button v-if="!t.archived" @click="openTask(t.id)">Open</Button>
              <Button v-if="!t.archived" @click="archiveTask(t)">Archive</Button>
              <Button v-else @click="restoreTask(t)">Restore</Button>
              <Button variant="danger" @click="deleteTask(t)">Delete</Button>
            </template>
          </Table>
          <p v-if="!shown.length" class="muted">No tasks.</p>
          <BulkDeleteDialog
            v-if="bulkDeleting"
            :tasks="selected"
            @close="bulkDeleting = false"
            @deleted="((bulkDeleting = false), (picked = new Set()))"
          />
        </div>
      </template>
      <template #worktrees>
        <div class="body">
          <div v-if="worktrees.some((w) => w.prunable)" class="toolbar">
            <Button @click="api.pruneWorktrees(projectId).then(loadWorktrees).catch(report)">
              Prune missing worktrees
            </Button>
          </div>
          <Table
            :columns="WORKTREE_COLUMNS"
            :rows="worktrees"
            row-key="path"
            size="sm"
            hover="clickable"
          >
            <template #cell-path="{ row: w }">
              <span :title="w.path">{{ w.path }}</span>
              <span v-if="w.is_main" class="flag">main</span>
              <span v-if="w.locked" class="flag">locked</span>
              <span v-if="w.prunable" class="flag">missing</span>
            </template>
            <template #cell-branch="{ row: w }">{{
              w.branch ?? `detached @ ${w.head.slice(0, 7)}`
            }}</template>
            <template #cell-base="{ row: w }">{{ w.base_branch ?? '—' }}</template>
            <template #cell-size="{ row: w }">{{ formatSize(sizes[w.path]) }}</template>
            <template #cell-task="{ row: w }">{{
              w.task_ids.length ? w.task_ids.map(taskTitle).join(', ') : '—'
            }}</template>
            <template #cell-actions="{ row: w }">
              <Button v-if="!w.prunable" @click="revealItemInDir(w.path).catch(report)">
                Reveal
              </Button>
              <Button v-if="worktreeActions(w).open" @click="openTask(w.task_ids[0])">
                Open task
              </Button>
              <Button v-if="worktreeActions(w).remove" variant="danger" @click="removeWorktree(w)">
                Remove
              </Button>
            </template>
          </Table>
        </div>
      </template>
      <template #settings>
        <div class="body">
          <Field label="Workspace">
            <Select
              v-model="workspaceId"
              :options="state.workspaces.map((w) => ({ value: w.id, label: w.name }))"
              @update:model-value="moveToWorkspace"
            />
          </Field>
          <Field label="Default base branch">
            <Select
              :model-value="staleDefault(branches) ? '' : (project?.default_base ?? '')"
              :options="[
                {
                  value: '',
                  label: `Automatic${branches?.automatic ? ` (${branches.automatic})` : ''}`,
                },
                ...(branches?.branches ?? []).map((b) => ({ value: b, label: b })),
              ]"
              :disabled="!branches"
              @update:model-value="setDefaultBase"
            />
          </Field>
          <p v-if="staleDefault(branches)" class="muted">
            {{ staleDefault(branches) }} no longer exists — using Automatic.
          </p>
          <p v-if="branches?.fetch_error" class="muted">
            Couldn't fetch origin — branches may be stale.
          </p>
        </div>
      </template>
    </Tabs>
    <p v-if="!project" class="muted">This project no longer exists.</p>
  </section>
</template>

<style scoped>
.project-page {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}
.tabs {
  flex: 1;
}
.body {
  height: 100%;
  overflow: auto;
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
:deep(.mono) {
  font-family: ui-monospace, Menlo, monospace;
  font-size: 12px;
}
:deep(.path) {
  max-width: 360px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
:deep(.num) {
  white-space: nowrap;
}
:deep(.actions) {
  white-space: nowrap;
  text-align: right;
}
:deep(.actions) button {
  margin-left: 4px;
}
.flag {
  margin-left: 6px;
  font-size: 11px;
  padding: 0 5px;
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--muted);
}
:deep(.pick) {
  width: 1%;
}
.bulk {
  display: flex;
  align-items: center;
  gap: 8px;
}
</style>
