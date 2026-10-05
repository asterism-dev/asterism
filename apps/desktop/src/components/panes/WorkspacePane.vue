<script setup lang="ts">
import { computed } from 'vue';
import { state } from '../../store';
import { workspaceEpoch } from '../../dock/workspace';
import WorkspaceDock from '../WorkspaceDock.vue';

defineProps<{ params: unknown }>();
const task = computed(() => state.tasks.find((t) => t.id === state.selectedTaskId) ?? null);
</script>

<template>
  <div class="pane">
    <WorkspaceDock v-if="task" :key="`${task.id}-${workspaceEpoch}`" :task="task" />
    <p v-else class="empty">Select a task, or create one with + Task.</p>
  </div>
</template>
