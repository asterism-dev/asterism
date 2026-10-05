<script setup lang="ts">
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { computed } from 'vue';
import { errorMessage } from '../api';
import { layout, toggleRightPanel } from '../layout';
import { state, toast } from '../store';

const task = computed(() => state.tasks.find((t) => t.id === state.selectedTaskId) ?? null);
const repo = computed(() => state.projects.find((p) => p.id === task.value?.project_id)?.name ?? '');

function copyBranch() {
  if (!task.value) return;
  navigator.clipboard.writeText(task.value.branch).then(() => toast('Branch copied')).catch(() => {});
}
</script>

<template>
  <header class="top-bar">
    <button class="icon" :aria-pressed="layout.leftOpen" title="Toggle sidebar" @click="layout.leftOpen = !layout.leftOpen">◧</button>
    <div class="location">
      <template v-if="task">
        <span class="repo">{{ repo }}</span>
        <span class="muted">/</span>
        <button class="branch" title="Copy branch name" @click="copyBranch">{{ task.branch }}</button>
      </template>
    </div>
    <button v-if="task" @click="revealItemInDir(task.worktree_path).catch((e) => toast(errorMessage(e)))">Reveal worktree</button>
    <button :class="{ active: layout.rightOpen && layout.rightPanel === 'diff' }" @click="toggleRightPanel('diff')">Diff</button>
    <button :class="{ active: layout.rightOpen && layout.rightPanel === 'activity' }" @click="toggleRightPanel('activity')">Activity Monitor</button>
    <button class="icon" :aria-pressed="layout.rightOpen" title="Toggle right panel" @click="layout.rightOpen = !layout.rightOpen">◨</button>
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
