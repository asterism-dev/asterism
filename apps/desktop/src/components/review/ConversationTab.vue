<script setup lang="ts">
import { computed, ref } from 'vue';
import { api } from '../../api';
import { codeExcerpt, formatTime, splitPatch, timeline } from '../../review';
import type { ReviewResult, ReviewThread } from '../../types';
import CommentEditor from './CommentEditor.vue';
import ThreadView from './ThreadView.vue';

const props = defineProps<{ taskId: number; review: ReviewResult; selected: Set<string> }>();
const emit = defineEmits<{
  select: [id: string, on: boolean];
  toAgent: [threads: ReviewThread[]];
}>();
const FILTERS = ['open', 'resolved', 'local', 'all'] as const;
const filter = ref<(typeof FILTERS)[number]>('all');
const isPr = computed(() => props.review.source === 'pr');
const patches = computed(
  () => new Map(splitPatch(props.review.patch).map((f) => [f.path, f.patch])),
);
const shows = (t: ReviewThread) =>
  filter.value === 'open'
    ? !t.resolved
    : filter.value === 'resolved'
      ? t.resolved
      : filter.value === 'local'
        ? t.local
        : true;
const entries = computed(() =>
  timeline(props.review).filter((e) =>
    e.kind === 'thread' ? shows(e.thread) : ['open', 'all'].includes(filter.value),
  ),
);
const STATES: Record<string, string> = {
  approved: 'approved these changes',
  changes_requested: 'requested changes',
  commented: 'reviewed',
  dismissed: 'had a review dismissed',
};
const excerpt = (t: ReviewThread) =>
  t.outdated ? [] : codeExcerpt(patches.value.get(t.path) ?? '', t.side, t.line);
const addComment = async (body: string) => {
  await api.reviewAddComment(props.taskId, body);
};
</script>

<template>
  <div class="conversation">
    <article v-if="isPr" class="card description">
      <header>
        <strong>@{{ review.author }}</strong>
        <span class="muted"> opened this pull request</span>
      </header>
      <p class="body">{{ review.body || 'No description provided.' }}</p>
    </article>
    <div class="filters">
      <label v-for="f in FILTERS" :key="f">
        <input v-model="filter" type="radio" :value="f" />
        {{ f }}
      </label>
    </div>
    <template v-for="e in entries" :key="e.kind + (e.kind === 'item' ? e.item.id : e.thread.id)">
      <article v-if="e.kind === 'item'" class="card">
        <header>
          <strong>@{{ e.item.author }}</strong>
          <span class="muted">
            {{
              e.item.kind === 'review' ? (STATES[e.item.state ?? ''] ?? 'reviewed') : 'commented'
            }}
            · {{ formatTime(e.item.created_at) }}
          </span>
        </header>
        <p v-if="e.item.body" class="body">{{ e.item.body }}</p>
      </article>
      <article v-else class="card">
        <header class="muted">
          {{ e.thread.path }}:{{ e.thread.line }}<span v-if="e.thread.outdated"> · outdated</span>
        </header>
        <pre v-if="excerpt(e.thread).length" class="excerpt"><code
          v-for="(l, i) in excerpt(e.thread)"
          :key="i"
          :class="l.tag === '+' ? 'add' : l.tag === '-' ? 'del' : ''"
        >{{ l.tag }}{{ l.text }}
</code></pre>
        <ThreadView
          :task-id="taskId"
          :thread="e.thread"
          :can-publish="isPr && review.reviews_supported"
          :pending-review="!!review.pending_review"
          :selected="selected.has(e.thread.id)"
          @select="(s) => emit('select', e.thread.id, s)"
          @to-agent="(th) => emit('toAgent', [th])"
        />
      </article>
    </template>
    <p v-if="!entries.length" class="muted">No comments.</p>
    <CommentEditor
      v-if="isPr && review.reviews_supported"
      :buttons="[{ label: 'Comment', target: 'single' }]"
      placeholder="Add a comment to the pull request"
      :submit="addComment"
    />
  </div>
</template>

<style scoped>
.conversation {
  flex: 1;
  overflow: auto;
  padding: 10px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-width: 900px;
}
.card {
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--panel);
}
.card > header {
  padding: 6px 10px;
  border-bottom: 1px solid var(--border);
  background: var(--select);
  font-size: 12px;
}
.body {
  white-space: pre-wrap;
  margin: 0;
  padding: 8px 10px;
}
.filters {
  display: flex;
  gap: 10px;
  font-size: 12px;
}
.filters label {
  display: flex;
  gap: 4px;
  align-items: center;
}
.filters input {
  width: auto;
}
.excerpt {
  margin: 0;
  padding: 6px 10px;
  font-size: 12px;
  overflow: auto;
  border-bottom: 1px solid var(--border);
}
.excerpt code {
  display: block;
}
.excerpt .add {
  background: rgba(45, 164, 78, 0.15);
}
.excerpt .del {
  background: rgba(196, 59, 59, 0.15);
}
</style>
