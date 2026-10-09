<script setup lang="ts">
import { computed, defineAsyncComponent } from 'vue';
import { state } from '../../store';

// Lazy so the diff library and its highlighter grammars stay out of the main chunk.
const ReviewView = defineAsyncComponent(() => import('../review/ReviewView.vue'));

defineProps<{ params: unknown }>();
const task = computed(() => state.tasks.find((t) => t.id === state.selectedTaskId) ?? null);
</script>

<template>
  <div class="pane">
    <ReviewView v-if="task" :key="task.id" :task="task" />
    <p v-else class="empty">Select a task to review its changes.</p>
  </div>
</template>
