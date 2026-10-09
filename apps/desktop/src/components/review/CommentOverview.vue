<script setup lang="ts">
import { computed, ref } from 'vue';
import type { ReviewResult, ReviewThread } from '../../types';
import ThreadView from './ThreadView.vue';

const props = defineProps<{ taskId: number; review: ReviewResult; selected: Set<string> }>();
const emit = defineEmits<{
  toAgent: [threads: ReviewThread[]];
  select: [id: string, selected: boolean];
}>();
const FILTERS = ['open', 'resolved', 'local', 'all'] as const;
const filter = ref<(typeof FILTERS)[number]>('open');
const shown = computed(() =>
  props.review.threads.filter((t) =>
    filter.value === 'open'
      ? !t.resolved
      : filter.value === 'resolved'
        ? t.resolved
        : filter.value === 'local'
          ? t.local
          : true,
  ),
);
</script>

<template>
  <div class="overview">
    <div class="filters">
      <label v-for="f in FILTERS" :key="f">
        <input v-model="filter" type="radio" :value="f" />
        {{ f }}
      </label>
    </div>
    <div v-for="c in review.conversation" :key="c.id" class="comment">
      <strong>@{{ c.author }}</strong> <span class="muted">{{ c.created_at }}</span>
      <p class="body">{{ c.body }}</p>
    </div>
    <ThreadView
      v-for="t in shown"
      :key="t.id"
      :task-id="taskId"
      :thread="t"
      :can-publish="review.source === 'pr' && review.reviews_supported"
      :selected="selected.has(t.id)"
      @select="(s) => emit('select', t.id, s)"
      @to-agent="(th) => emit('toAgent', [th])"
    />
    <p v-if="!shown.length && !review.conversation.length" class="muted">No comments.</p>
  </div>
</template>

<style scoped>
.overview {
  flex: 1;
  overflow: auto;
  padding: 8px 14px;
}
.body {
  white-space: pre-wrap;
}
.filters {
  display: flex;
  gap: 10px;
  padding: 6px 8px;
}
.filters label {
  display: flex;
  gap: 4px;
  align-items: center;
}
.filters input {
  width: auto;
}
</style>
