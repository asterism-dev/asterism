<script setup lang="ts">
import { ask } from '@tauri-apps/plugin-dialog';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { Activity, FolderOpen, GitCompare, Lock, LockOpen, PanelLeftClose, PanelLeftOpen, RotateCcw, Settings } from 'lucide-vue-next';
import { computed } from 'vue';
import { errorMessage } from '../api';
import { floatUnlocked, mainApi, paneState, resetLayout as resetMainLayout, setFloatUnlocked, togglePane } from '../dock/main';
import { sidebar } from '../dock/sidebar';
import { state, toast } from '../store';

const task = computed(() => state.tasks.find((t) => t.id === state.selectedTaskId) ?? null);
const pageProject = computed(() => (state.projectPage === null ? null : state.projects.find((p) => p.id === state.projectPage) ?? null));
const repo = computed(() => state.projects.find((p) => p.id === task.value?.project_id)?.name ?? '');

async function resetLayout() {
  const confirmed = await ask('Reset this task\'s layout to the default?', { title: 'Reset layout', kind: 'warning' });
  if (confirmed) resetMainLayout();
}

function copyBranch() {
  if (!task.value) return;
  navigator.clipboard.writeText(task.value.branch).then(() => toast('Branch copied')).catch(() => {});
}
</script>

<template>
  <header class="top-bar">
    <button class="icon" :aria-pressed="sidebar.open" :title="sidebar.open ? 'Hide projects' : 'Show projects'" @click="sidebar.open = !sidebar.open">
      <PanelLeftClose v-if="sidebar.open" /><PanelLeftOpen v-else />
    </button>
    <div class="location">
      <span v-if="pageProject" class="repo">{{ pageProject.name }}</span>
      <template v-else-if="task">
        <span class="repo">{{ repo }}</span>
        <span class="muted">/</span>
        <button class="branch" title="Copy branch name" @click="copyBranch">{{ task.branch }}</button>
      </template>
    </div>
    <template v-if="state.projectPage === null">
    <button v-if="task" @click="revealItemInDir(task.worktree_path).catch((e) => toast(errorMessage(e)))"><FolderOpen />Reveal worktree</button>
    <button :disabled="!mainApi" :class="{ active: paneState('diff') === 'front' }" @click="togglePane('diff')"><GitCompare />Diff</button>
    <button :disabled="!mainApi" :class="{ active: paneState('activity') === 'front' }" @click="togglePane('activity')"><Activity />Activity Monitor</button>
    <button
      class="icon"
      :disabled="!mainApi"
      :aria-pressed="floatUnlocked"
      :title="floatUnlocked ? 'Free mode on: tabs can float (click to lock)' : 'Free mode off (click to let tabs float)'"
      @click="setFloatUnlocked(!floatUnlocked)"
    ><LockOpen v-if="floatUnlocked" /><Lock v-else /></button>
    <button class="icon" :disabled="!mainApi" title="Reset layout" aria-label="Reset layout" @click="resetLayout"><RotateCcw /></button>
    </template>
    <button class="icon" title="Settings" aria-label="Settings" @click="state.settingsOpen = true"><Settings /></button>
  </header>
</template>

<style scoped>
.top-bar { display: flex; align-items: center; gap: 6px; padding: 6px 10px; border-bottom: 1px solid var(--border); background: var(--panel); min-height: 40px; }
.location { flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px; overflow: hidden; white-space: nowrap; }
.repo { font-weight: 600; }
.branch { border: 0; background: transparent; padding: 2px 4px; font-family: ui-monospace, Menlo, monospace; font-size: 12px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; }
.branch:hover { color: var(--text); }
.icon { border: 0; background: transparent; color: var(--muted); padding: 2px 6px; }
.icon[aria-pressed="true"] { color: var(--text); }
button.active { background: var(--select); border-color: var(--accent); }
</style>
