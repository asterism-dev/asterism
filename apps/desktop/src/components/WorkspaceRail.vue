<script setup lang="ts">
import { WorkspaceRail, type WorkspaceColor, type WorkspaceItem } from '@asterism-dev/lumen';
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
const items = computed<WorkspaceItem[]>(() =>
  state.workspaces.map((w) => ({
    id: String(w.id),
    name: w.name,
    label: w.label,
    color: (w.color || undefined) as WorkspaceColor | undefined,
    waiting: waiting.value[w.id],
  })),
);
const byId = (id: string) => state.workspaces.find((w) => String(w.id) === id);

// Persist continuously so the task open at quit is restored, not only on switch.
watch(
  () => state.selectedTaskId,
  (id) => {
    if (id === null || state.activeWorkspace === null) return;
    state.workspaceTasks[state.activeWorkspace] = id;
    saveWorkspaceTasks(state.workspaceTasks);
  },
);

function select(id: string) {
  const w = byId(id);
  if (!w) return;
  leaveSettings()
    .then((left) => left && switchWorkspace(state, w.id))
    .catch((e) => toast(errorMessage(e)));
}

function menu(id: string, e: MouseEvent) {
  const w = byId(id);
  if (!w) return;
  showMenu(e, [
    { label: 'Edit…', action: () => (dialog.value = { mode: 'rename', workspace: w }) },
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
  <WorkspaceRail
    :workspaces="items"
    :active="state.activeWorkspace === null ? null : String(state.activeWorkspace)"
    @select="select"
    @add="dialog = { mode: 'create' }"
    @contextmenu="menu"
  />
  <WorkspaceDialog v-if="dialog" v-bind="dialog" @close="dialog = null" />
</template>
