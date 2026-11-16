<script setup lang="ts">
import { Button, Popover } from '@asterism-dev/lumen';
import { ask } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  Activity,
  ChevronDown,
  Ellipsis,
  FolderTree,
  GitCompare,
  Gauge,
  LoaderCircle,
  PanelLeftClose,
  PanelLeftOpen,
} from '@lucide/vue';
import { computed, ref, watch } from 'vue';
import { anyRunning, formatDiffCount } from '../activity';
import { api, errorMessage } from '../api';
import {
  floatUnlocked,
  mainApi,
  paneState,
  resetLayout as resetMainLayout,
  setFloatUnlocked,
  togglePane,
} from '../dock/main';
import { sidebar } from '../dock/sidebar';
import { isTreeOpen, setTreeOpen } from '../fileTreeBar';
import { withHint } from '../shortcuts';
import { showMenu, state, taskStatus, toast } from '../store';
import ActivityMenu from './ActivityMenu.vue';
import OpenInButton from './OpenInButton.vue';
import PrBadge from './PrBadge.vue';

const task = computed(() => state.tasks.find((t) => t.id === state.selectedTaskId) ?? null);
const pageProject = computed(() =>
  state.projectPage === null
    ? null
    : (state.projects.find((p) => p.id === state.projectPage) ?? null),
);
const repo = computed(
  () => state.projects.find((p) => p.id === task.value?.project_id)?.name ?? '',
);
const activityOpen = ref(false);
const running = computed(() => anyRunning(state.activities));
const added = ref(0);

watch(
  [
    () => task.value?.id,
    () => (task.value ? state.reviewVersion[task.value.id] : undefined),
    () => (task.value ? taskStatus(state, task.value.id) !== 'working' : undefined),
  ],
  ([id], [prevId]) => {
    if (id !== prevId) added.value = 0;
    if (id === undefined) return;
    api
      .diffStat(id)
      .then((stat) => {
        if (task.value?.id === id) added.value = stat.added;
      })
      .catch(() => {});
  },
  { immediate: true },
);

const report = (e: unknown) => toast(errorMessage(e));

async function resetLayout() {
  const confirmed = await ask("Reset this task's layout to the default?", {
    title: 'Reset layout',
    kind: 'warning',
  });
  if (confirmed) resetMainLayout();
}

function copyBranch() {
  if (!task.value) return;
  navigator.clipboard
    .writeText(task.value.branch)
    .then(() => toast('Branch copied'))
    .catch(() => {});
}

function branchMenu(e: MouseEvent) {
  showMenu(e, [{ label: 'Copy branch name', action: copyBranch }]);
}

function moreMenu(e: MouseEvent) {
  showMenu(e, [
    {
      label: floatUnlocked.value ? 'Lock tabs' : 'Let tabs float',
      action: () => setFloatUnlocked(!floatUnlocked.value),
    },
    { label: 'Reset layout…', action: () => void resetLayout() },
  ]);
}
</script>

<template>
  <header class="top-bar">
    <Button
      variant="ghost"
      :icon="sidebar.open ? PanelLeftClose : PanelLeftOpen"
      :aria-pressed="sidebar.open"
      :title="withHint(sidebar.open ? 'Hide projects' : 'Show projects', 'toggleSidebar')"
      @click="sidebar.open = !sidebar.open"
    />
    <div class="location">
      <span v-if="pageProject" class="repo">{{ pageProject.name }}</span>
      <template v-else-if="task">
        <span class="repo">{{ repo }}</span>
        <span class="muted">/</span>
        <Button variant="ghost" size="sm" class="branch" :title="task.branch" @click="branchMenu">
          <span class="branch-name">{{ task.branch }}</span
          ><ChevronDown :size="14" />
        </Button>
        <a
          v-if="task.issue"
          class="chip issue"
          :href="task.issue.url"
          :title="task.issue.url"
          @click.prevent="openUrl(task.issue.url).catch(report)"
          >{{ task.issue.key }}</a
        >
        <PrBadge v-if="state.prs[task.id]" class="chip" :pr="state.prs[task.id]!" />
      </template>
    </div>
    <Popover v-model:open="activityOpen" align="end" aria-label="Activity">
      <Button variant="ghost" :class="{ active: activityOpen }" title="Activity">
        <LoaderCircle v-if="running" class="spin" /><Activity v-else />Activity
      </Button>
      <template #content><ActivityMenu /></template>
    </Popover>
    <template v-if="state.projectPage === null">
      <OpenInButton v-if="task" :path="task.worktree_path" />
      <Button
        v-if="task"
        variant="ghost"
        :icon="FolderTree"
        :aria-pressed="isTreeOpen(task.id)"
        :class="{ active: isTreeOpen(task.id) }"
        title="Toggle file tree"
        @click="setTreeOpen(task.id, !isTreeOpen(task.id))"
      >
        Files
      </Button>
      <Button
        variant="ghost"
        :icon="GitCompare"
        :disabled="!mainApi"
        :class="{ active: paneState('diff') === 'front' }"
        :title="withHint('Toggle review', 'toggleDiff')"
        @click="togglePane('diff')"
      >
        Review<span v-if="formatDiffCount(added)" class="added">{{ formatDiffCount(added) }}</span>
      </Button>
      <Button
        variant="ghost"
        :icon="Gauge"
        :disabled="!mainApi"
        :class="{ active: paneState('activity') === 'front' }"
        @click="togglePane('activity')"
      >
        Resources
      </Button>
      <span class="separator" />
      <Button
        variant="ghost"
        :icon="Ellipsis"
        :disabled="!mainApi"
        title="Layout"
        aria-label="Layout"
        @click="moreMenu"
      />
    </template>
  </header>
</template>

<style scoped>
.top-bar {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 6px 10px;
  border-bottom: 1px solid var(--border);
  background: var(--panel);
  min-height: 40px;
}
.lm-button {
  color: var(--muted);
}
.lm-button.active {
  background: var(--select);
  color: var(--text);
}
.location {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  margin-left: 4px;
  overflow: hidden;
  white-space: nowrap;
}
.repo {
  font-weight: 600;
}
.branch {
  min-width: 0;
  gap: 2px;
  font-family: ui-monospace, Menlo, monospace;
  font-size: 12px;
}
.branch-name {
  overflow: hidden;
  text-overflow: ellipsis;
}
.chip {
  flex: none;
  font-size: 12px;
  padding: 1px 6px;
  border-radius: 6px;
  background: var(--select);
  text-decoration: none;
}
.issue {
  color: var(--muted);
}
.added {
  color: var(--ws-green);
  font-size: 12px;
}
.separator {
  width: 1px;
  height: 18px;
  margin: 0 4px;
  background: var(--border);
}
</style>
