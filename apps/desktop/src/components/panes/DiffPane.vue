<script setup lang="ts">
import { computed } from 'vue';
import { state } from '../../store';
import DiffView from '../DiffView.vue';

defineProps<{ params: unknown }>();
const task = computed(() => state.tasks.find((t) => t.id === state.selectedTaskId) ?? null);
</script>

<template>
  <div class="pane">
    <DiffView v-if="task" :key="task.id" :task="task" />
    <p v-else class="empty">Select a task to see its diff.</p>
  </div>
</template>
