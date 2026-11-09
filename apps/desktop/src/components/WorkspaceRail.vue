<script setup lang="ts">
import { Plus } from '@lucide/vue';
import { computed, ref, watch } from 'vue';
import { errorMessage } from '../api';
import { leaveSettings } from '../settingsGuard';
import { showMenu, state, switchWorkspace, toast, waitingSessions } from '../store';
import { saveWorkspaceTasks, waitingByWorkspace } from '../workspaces';
import type { Workspace } from '../types';
import WorkspaceDialog from './WorkspaceDialog.vue';

const dialog = ref<{ mode: 'create' | 'rename' | 'delete'; workspace?: Workspace } | null>(null);
const waiting = computed(() =>
  waitingByWorkspace(state.projects, state.tasks, waitingSessions(state)),
);

// Persist continuously so the task open at quit is restored, not only on switch.
watch(
  () => state.selectedTaskId,
  (id) => {
    if (id === null || state.activeWorkspace === null) return;
    state.workspaceTasks[state.activeWorkspace] = id;
    saveWorkspaceTasks(state.workspaceTasks);
  },
);

function select(w: Workspace) {
  leaveSettings()
    .then((left) => left && switchWorkspace(state, w.id))
    .catch((e) => toast(errorMessage(e)));
}

function menu(e: MouseEvent, w: Workspace) {
  showMenu(e, [
    { label: 'Rename…', action: () => (dialog.value = { mode: 'rename', workspace: w }) },
    ...(state.workspaces.length > 1
      ? [
          {
            label: 'Delete…',
            danger: true,
            action: () => (dialog.value = { mode: 'delete', workspace: w }),
          },
        ]
      : []),
  ]);
}
</script>

<template>
  <nav class="rail" aria-label="Workspaces">
    <button
      v-for="w in state.workspaces"
      :key="w.id"
      type="button"
      class="tile"
      :class="{ active: state.activeWorkspace === w.id }"
      :title="w.name"
      :aria-label="
        !waiting[w.id] || state.activeWorkspace === w.id
          ? w.name
          : `${w.name}, ${waiting[w.id]} waiting`
      "
      :aria-pressed="state.activeWorkspace === w.id"
      @click="select(w)"
      @contextmenu="menu($event, w)"
    >
      {{ w.label }}
      <span v-if="state.activeWorkspace !== w.id && waiting[w.id]" class="badge">{{
        waiting[w.id]
      }}</span>
    </button>
    <button
      type="button"
      class="tile add"
      title="New workspace"
      aria-label="New workspace"
      @click="dialog = { mode: 'create' }"
    >
      <Plus :size="16" />
    </button>
  </nav>
  <WorkspaceDialog v-if="dialog" v-bind="dialog" @close="dialog = null" />
</template>

<style scoped>
.rail {
  flex: none;
  width: 44px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 8px 0;
  border-right: 1px solid var(--border);
  overflow-y: auto;
}
.tile {
  position: relative;
  width: 32px;
  height: 32px;
  padding: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 600;
  font-size: 12px;
  border-radius: 8px;
}
.tile.active {
  background: var(--select);
  border-color: var(--muted);
}
.tile.add {
  margin-top: auto;
  color: var(--muted);
}
.badge {
  position: absolute;
  top: -4px;
  right: -4px;
  background: var(--waiting);
  color: #1d1f27;
  padding: 0 5px;
  border-radius: 10px;
  font-size: 11px;
}
</style>
