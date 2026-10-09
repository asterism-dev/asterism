<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { api, errorMessage } from '../../api';
import { splitPatch, threadsByFile } from '../../review';
import { state, taskStatus } from '../../store';
import type { ReviewEvent, ReviewResult, ReviewSource, ReviewThread, Task } from '../../types';
import CommentOverview from './CommentOverview.vue';
import FileDiff from './FileDiff.vue';
import SendToAgentDialog from './SendToAgentDialog.vue';

const props = defineProps<{ task: Task }>();
const pr = computed(() => state.prs[props.task.id] ?? null);
const source = ref<ReviewSource>(pr.value ? 'pr' : 'local');
const review = ref<ReviewResult | null>(null);
const error = ref<string | null>(null);
const loading = ref(false);
const split = ref(false);
const showOverview = ref(false);
const selected = ref(new Set<string>());
const sending = ref<ReviewThread[] | null>(null);
const submitOpen = ref(false);
const submitBody = ref('');
const submitError = ref<string | null>(null);
const submitting = ref(false);
let latestLoad = 0;

const files = computed(() => splitPatch(review.value?.patch ?? ''));
const byFile = computed(() => threadsByFile(review.value?.threads ?? []));
const viewed = computed(() => new Set(review.value?.viewed_files ?? []));
const openThreads = computed(() => (review.value?.threads ?? []).filter((t) => !t.resolved));

async function load() {
  const request = ++latestLoad;
  loading.value = true;
  try {
    const result = await api.review(props.task.id, source.value);
    if (request !== latestLoad) return;
    review.value = result;
    error.value = null;
  } catch (e) {
    if (request === latestLoad) error.value = errorMessage(e);
  } finally {
    if (request === latestLoad) loading.value = false;
  }
}

function select(id: string, on: boolean) {
  const next = new Set(selected.value);
  if (on) next.add(id);
  else next.delete(id);
  selected.value = next;
}

function sendSelection() {
  const threads = review.value?.threads ?? [];
  sending.value = selected.value.size
    ? threads.filter((t) => selected.value.has(t.id))
    : openThreads.value;
}

async function submit(event: ReviewEvent) {
  submitting.value = true;
  submitError.value = null;
  try {
    await api.reviewSubmit(props.task.id, event, submitBody.value);
    submitOpen.value = false;
    submitBody.value = '';
  } catch (e) {
    submitError.value = errorMessage(e);
  } finally {
    submitting.value = false;
  }
}

watch(source, () => {
  selected.value = new Set();
  load();
});
watch(() => state.reviewVersion[props.task.id], load);
// ponytail: reloads when the task's sessions settle instead of watching the worktree; add a daemon fs-watch event if agents edit while idle.
watch(
  () => taskStatus(state, props.task.id),
  (status) => {
    if (status !== 'working' && source.value === 'local') load();
  },
);
const poll = setInterval(() => {
  if (source.value === 'pr') load();
}, 60_000);
onMounted(load);
onUnmounted(() => clearInterval(poll));
</script>

<template>
  <div class="review-view">
    <div class="toolbar">
      <div v-if="pr" class="segmented">
        <button :class="{ on: source === 'pr' }" @click="source = 'pr'">PR #{{ pr.number }}</button>
        <button :class="{ on: source === 'local' }" @click="source = 'local'">Local</button>
      </div>
      <span v-if="review?.local_ahead" class="hint">
        The worktree differs from the pull request.
      </span>
      <span v-if="files.length" class="muted">
        {{ files.filter((f) => viewed.has(f.path)).length }} / {{ files.length }} files viewed
      </span>
      <span class="spacer" />
      <label><input v-model="split" type="checkbox" /> Side by side</label>
      <button @click="showOverview = !showOverview">
        {{ showOverview ? 'Files' : 'All comments' }}
      </button>
      <button :disabled="!review?.threads.length" @click="sendSelection">
        Send to agent ({{ selected.size || openThreads.length }})
      </button>
      <button :disabled="loading" @click="load">Refresh</button>
    </div>
    <div v-if="review?.source === 'pr' && review.reviews_supported" class="review-bar">
      <span v-if="review.pending_review">
        Review: {{ review.pending_review.comments }} comment(s) pending
      </span>
      <button @click="submitOpen = !submitOpen">
        {{ review.pending_review ? 'Finish review' : 'Review changes' }} ▾
      </button>
      <div v-if="submitOpen" class="submit-box">
        <textarea v-model="submitBody" rows="3" placeholder="Leave a comment" />
        <p v-if="submitError" class="error">{{ submitError }}</p>
        <div class="actions">
          <button :disabled="submitting" @click="submit('comment')">Comment</button>
          <button :disabled="submitting" @click="submit('approve')">Approve</button>
          <button :disabled="submitting" @click="submit('request_changes')">Request changes</button>
        </div>
      </div>
    </div>
    <p v-if="error" class="error message">{{ error }} <button @click="load">Retry</button></p>
    <p v-else-if="review && !files.length" class="muted message">
      No changes against {{ task.base_branch }}.
    </p>
    <div v-if="review" class="body">
      <CommentOverview
        v-if="showOverview"
        :task-id="task.id"
        :review="review"
        :selected="selected"
        @select="select"
        @to-agent="(t) => (sending = t)"
      />
      <template v-else>
        <nav class="file-list">
          <a
            v-for="f in files"
            :key="f.path"
            :href="'#file-' + f.path"
            :class="{ done: viewed.has(f.path) }"
          >
            {{ viewed.has(f.path) ? '✓' : '·' }} {{ f.path }}
            <span v-if="byFile.get(f.path)?.length" class="count">
              {{ byFile.get(f.path)?.length }}
            </span>
          </a>
        </nav>
        <div class="files">
          <FileDiff
            v-for="f in files"
            :key="f.path + (viewed.has(f.path) ? ':v' : '')"
            :task-id="task.id"
            :file="f"
            :review="review"
            :threads="byFile.get(f.path) ?? []"
            :split="split"
            :viewed="viewed.has(f.path)"
            :selected="selected"
            @select="select"
            @to-agent="(t) => (sending = t)"
          />
        </div>
      </template>
    </div>
    <SendToAgentDialog
      v-if="sending && review"
      :task-id="task.id"
      :source="review.source"
      :threads="sending"
      @close="sending = null"
    />
  </div>
</template>

<style scoped>
.review-view {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
}
.toolbar,
.review-bar {
  display: flex;
  gap: 10px;
  align-items: center;
  padding: 8px 14px;
  flex-wrap: wrap;
}
.toolbar label {
  display: flex;
  gap: 6px;
  align-items: center;
}
.toolbar input {
  width: auto;
}
.spacer {
  flex: 1;
}
.segmented button.on {
  font-weight: 600;
  background: var(--select);
}
.message {
  padding: 0 14px;
}
.body {
  flex: 1;
  min-height: 0;
  display: flex;
}
.file-list {
  width: 220px;
  flex: none;
  overflow: auto;
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 12px;
  border-right: 1px solid var(--border);
}
.file-list a {
  color: inherit;
  text-decoration: none;
  overflow-wrap: anywhere;
}
.file-list .done {
  opacity: 0.6;
}
.files {
  flex: 1;
  min-width: 0;
  overflow: auto;
  padding: 8px 14px;
}
.count {
  margin-left: 4px;
  font-size: 11px;
  color: var(--muted);
}
.submit-box {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.submit-box .actions {
  display: flex;
  gap: 6px;
  justify-content: flex-end;
}
</style>
