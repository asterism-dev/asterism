<script setup lang="ts">
import { computed } from 'vue';
import { state } from '../../store';
import TaskView from '../TaskView.vue';

defineProps<{ params: unknown }>();
const task = computed(() => state.tasks.find((t) => t.id === state.selectedTaskId) ?? null);
</script>

<template>
  <div class="pane">
    <TaskView v-if="task" :key="task.id" :task="task" />
    <p v-else class="empty">Select a task, or create one with + Task.</p>
  </div>
</template>
