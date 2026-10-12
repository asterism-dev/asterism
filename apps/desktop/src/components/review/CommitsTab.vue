<script setup lang="ts">
import { computed } from 'vue';
import { formatTime, groupByDay } from '../../review';
import type { ReviewCommitsResult, ReviewSource } from '../../types';

const props = defineProps<{
  commits: ReviewCommitsResult;
  checks: Record<string, string>;
  source: ReviewSource;
  current: string | null | undefined;
}>();
const emit = defineEmits<{ open: [sha: string | null] }>();
const groups = computed(() => groupByDay(props.commits.commits));
const ICON: Record<string, string> = {
  success: '✓',
  failure: '✗',
  error: '✗',
  pending: '●',
  expected: '●',
};
const dayLabel = (day: string) =>
  new Date(day + 'T12:00:00').toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
</script>

<template>
  <div class="commits">
    <button
      v-if="source === 'local' && commits.uncommitted"
      class="row"
      :class="{ on: current === null }"
      @click="emit('open', null)"
    >
      <span class="subject">Uncommitted changes</span>
    </button>
    <section v-for="g in groups" :key="g.day">
      <h4 class="muted">Commits on {{ dayLabel(g.day) }}</h4>
      <button
        v-for="c in g.commits"
        :key="c.sha"
        class="row"
        :class="{ on: current === c.sha }"
        @click="emit('open', c.sha)"
      >
        <span class="subject">{{ c.subject }}</span>
        <span class="muted meta">{{ c.author }} · {{ formatTime(c.date) }}</span>
        <span v-if="checks[c.sha]" class="state" :class="checks[c.sha]">{{
          ICON[checks[c.sha]] ?? '●'
        }}</span>
        <code class="sha">{{ c.short }}</code>
      </button>
    </section>
    <p v-if="!commits.commits.length && !commits.uncommitted" class="muted">No commits.</p>
  </div>
</template>

<style scoped>
.commits {
  flex: 1;
  overflow: auto;
  padding: 10px 14px;
  max-width: 900px;
}
h4 {
  margin: 12px 0 6px;
  font-size: 12px;
  font-weight: 600;
}
.row {
  display: flex;
  width: 100%;
  gap: 10px;
  align-items: center;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-top: none;
  background: var(--panel);
  text-align: left;
  cursor: pointer;
}
section .row:first-of-type,
.commits > .row {
  border-top: 1px solid var(--border);
}
.row.on,
.row:hover {
  background: var(--select);
}
.subject {
  flex: 1;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.meta {
  font-size: 12px;
}
.sha {
  font-size: 12px;
  color: var(--muted);
}
.state.success {
  color: #2da44e;
}
.state.failure,
.state.error {
  color: var(--danger);
}
.state.pending,
.state.expected {
  color: var(--waiting);
}
</style>
