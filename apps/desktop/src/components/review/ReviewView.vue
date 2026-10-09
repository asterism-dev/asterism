<script setup lang="ts">
import { openUrl } from '@tauri-apps/plugin-opener';
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { api, errorMessage } from '../../api';
import { lastReviewTab, pollInterval, splitPatch, type ReviewTab } from '../../review';
import { state, taskStatus, toast } from '../../store';
import type {
  ReviewCommitsResult,
  ReviewResult,
  ReviewSource,
  ReviewThread,
  Task,
} from '../../types';
import ChecksTab from './ChecksTab.vue';
import CommitsTab from './CommitsTab.vue';
import ConversationTab from './ConversationTab.vue';
import FilesTab from './FilesTab.vue';
import ReviewSubmitBar from './ReviewSubmitBar.vue';
import ReviewTabs from './ReviewTabs.vue';
import SendToAgentDialog from './SendToAgentDialog.vue';

const props = defineProps<{ task: Task }>();
const pr = computed(() => state.prs[props.task.id] ?? null);
const source = ref<ReviewSource>(pr.value ? 'pr' : 'local');
const review = ref<ReviewResult | null>(null);
const commits = ref<ReviewCommitsResult | null>(null);
const error = ref<string | null>(null);
const loading = ref(false);
const tab = ref<ReviewTab>(lastReviewTab.get(props.task.id) ?? 'files');
// undefined: all changes; null: uncommitted changes; string: one commit.
const commit = ref<string | null | undefined>(undefined);
const commitPatch = ref<string | null>(null);
const commitError = ref<string | null>(null);
const selected = ref(new Set<string>());
const sending = ref<{ threads: ReviewThread[]; prompt?: string } | null>(null);
let latestLoad = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
let disposed = false;

const isPr = computed(() => review.value?.source === 'pr');
const files = computed(() => splitPatch(review.value?.patch ?? ''));
const additions = computed(() => files.value.reduce((n, f) => n + f.additions, 0));
const deletions = computed(() => files.value.reduce((n, f) => n + f.deletions, 0));
const tabs = computed(() => {
  const r = review.value;
  const list: { id: ReviewTab; label: string; count: number }[] = [
    {
      id: 'conversation',
      label: 'Conversation',
      count: (r?.conversation.length ?? 0) + (r?.threads.length ?? 0),
    },
    {
      id: 'commits',
      label: 'Commits',
      count: (commits.value?.commits.length ?? 0) + (commits.value?.uncommitted ? 1 : 0),
    },
  ];
  if (isPr.value) list.push({ id: 'checks', label: 'Checks', count: r?.checks.length ?? 0 });
  list.push({ id: 'files', label: 'Files changed', count: files.value.length });
  return list;
});

function schedule() {
  clearTimeout(timer);
  if (!disposed && source.value === 'pr') timer = setTimeout(load, pollInterval(review.value));
}

async function load() {
  const request = ++latestLoad;
  loading.value = true;
  try {
    const [result, list] = await Promise.all([
      api.review(props.task.id, source.value),
      api.reviewCommits(props.task.id, source.value).catch(() => null),
    ]);
    if (request !== latestLoad) return;
    review.value = result;
    commits.value = list;
    // The daemon falls back to local when the forge has no reviews.
    source.value = result.source;
    if (result.source === 'local' && tab.value === 'checks') tab.value = 'files';
    if (commit.value === null) fetchCommitDiff(null);
    error.value = null;
  } catch (e) {
    if (request === latestLoad) error.value = errorMessage(e);
  } finally {
    if (request === latestLoad) {
      loading.value = false;
      schedule();
    }
  }
}

async function fetchCommitDiff(sha: string | null) {
  try {
    const result = await api.reviewCommitDiff(props.task.id, source.value, sha);
    if (commit.value === sha) {
      commitPatch.value = result.patch;
      commitError.value = null;
    }
  } catch (e) {
    if (commit.value === sha) commitError.value = errorMessage(e);
  }
}

function openCommit(sha: string | null) {
  commit.value = sha;
  commitPatch.value = null;
  commitError.value = null;
  tab.value = 'files';
  return fetchCommitDiff(sha);
}

function showAll() {
  commit.value = undefined;
  commitPatch.value = null;
  commitError.value = null;
}

function select(id: string, on: boolean) {
  const next = new Set(selected.value);
  if (on) next.add(id);
  else next.delete(id);
  selected.value = next;
}

function sendSelection() {
  const threads = review.value?.threads ?? [];
  sending.value = {
    threads: selected.value.size
      ? threads.filter((t) => selected.value.has(t.id))
      : threads.filter((t) => !t.resolved),
  };
}

function openExternal(url: string) {
  openUrl(url).catch((e) => toast(errorMessage(e)));
}

watch(tab, (t) => lastReviewTab.set(props.task.id, t));
watch(source, (next) => {
  if (review.value?.source === next) return;
  selected.value = new Set();
  showAll();
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
onMounted(load);
onUnmounted(() => {
  disposed = true;
  clearTimeout(timer);
});
</script>

<template>
  <div class="review-view">
    <header class="head">
      <h3 class="title">
        <template v-if="isPr && review">
          <span class="muted">#{{ review.pr }}</span> {{ review.title }}
          <button
            v-if="review.url"
            class="link"
            title="Open in browser"
            @click="openExternal(review.url)"
          >
            ↗
          </button>
        </template>
        <template v-else>{{ task.title }}</template>
      </h3>
      <span class="spacer" />
      <div v-if="pr && review?.reviews_supported" class="segmented">
        <button :class="{ on: source === 'pr' }" @click="source = 'pr'">PR #{{ pr.number }}</button>
        <button :class="{ on: source === 'local' }" @click="source = 'local'">Local</button>
      </div>
      <button :disabled="loading" @click="load">Refresh</button>
    </header>
    <p v-if="review?.local_ahead" class="hint">The worktree differs from the pull request.</p>
    <ReviewSubmitBar v-if="isPr && review?.reviews_supported" :task-id="task.id" :review="review" />
    <ReviewTabs
      v-if="review"
      :tabs="tabs"
      :active="tab"
      :additions="additions"
      :deletions="deletions"
      @select="(t) => (tab = t)"
    />
    <p v-if="error" class="error message">{{ error }} <button @click="load">Retry</button></p>
    <div v-if="review" class="content">
      <ConversationTab
        v-if="tab === 'conversation'"
        :task-id="task.id"
        :review="review"
        :selected="selected"
        @select="select"
        @to-agent="(t) => (sending = { threads: t })"
      />
      <CommitsTab
        v-else-if="tab === 'commits' && commits"
        :commits="commits"
        :checks="review.commit_checks"
        :source="review.source"
        :current="commit"
        @open="openCommit"
      />
      <ChecksTab
        v-else-if="tab === 'checks' && isPr"
        :task-id="task.id"
        :checks="review.checks"
        @to-agent="(prompt) => (sending = { threads: [], prompt })"
      />
      <template v-else-if="tab === 'files'">
        <p v-if="commitError" class="error message">
          {{ commitError }} <button @click="showAll">Show all changes</button>
        </p>
        <p v-else-if="commit !== undefined && commitPatch === null" class="muted message">
          Loading…
        </p>
        <FilesTab
          v-else
          :task-id="task.id"
          :review="review"
          :patch="commit === undefined ? review.patch : (commitPatch ?? '')"
          :commit="commit"
          :selected="selected"
          @select="select"
          @to-agent="(t) => (sending = { threads: t })"
          @send-selection="sendSelection"
          @show-all="showAll"
        />
      </template>
    </div>
    <SendToAgentDialog
      v-if="sending && review"
      :task-id="task.id"
      :source="review.source"
      :threads="sending.threads"
      :initial-prompt="sending.prompt"
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
.head {
  display: flex;
  gap: 10px;
  align-items: center;
  padding: 8px 14px;
}
.title {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.spacer {
  flex: 1;
}
.segmented button.on {
  font-weight: 600;
  background: var(--select);
}
.link {
  background: none;
  border: none;
  padding: 0 4px;
  cursor: pointer;
  color: var(--muted);
}
.hint,
.message {
  padding: 0 14px;
}
.content {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
</style>
