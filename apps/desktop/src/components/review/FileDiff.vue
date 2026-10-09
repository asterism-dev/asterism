<script setup lang="ts">
import { DiffModeEnum, DiffView, SplitSide } from '@git-diff-view/vue';
import '@git-diff-view/vue/styles/diff-view-pure.css';
import { computed, ref, shallowRef, watch } from 'vue';
import { api, errorMessage } from '../../api';
import { editorButtons, extendDataFor, type FileDiff } from '../../review';
import { activeTheme } from '../../theme';
import type { CommentTarget, ReviewResult, ReviewThread } from '../../types';
import CommentEditor from './CommentEditor.vue';
import ThreadView from './ThreadView.vue';

const props = defineProps<{
  taskId: number;
  file: FileDiff;
  review: ReviewResult;
  threads: ReviewThread[];
  split: boolean;
  viewed: boolean;
  selected: Set<string>;
}>();
const emit = defineEmits<{
  toAgent: [threads: ReviewThread[]];
  select: [id: string, selected: boolean];
}>();
const open = ref(!props.viewed);
const isViewed = ref(props.viewed);
const viewedError = ref<string | null>(null);

// The parser needs the `---`/`+++` header, so each file passes its whole `diff --git` chunk.
const diffData = () => ({
  oldFile: { fileName: props.file.path },
  newFile: { fileName: props.file.path },
  hunks: [props.file.patch],
});
// DiffView rebuilds its DiffFile and closes open comment editors whenever `data` changes, so only a new patch replaces it.
const data = shallowRef(diffData());
watch(
  () => props.file.patch,
  () => (data.value = diffData()),
);
const extend = computed(() => extendDataFor(props.threads));
const buttons = computed(() =>
  editorButtons(props.review.source, props.review.reviews_supported, !!props.review.pending_review),
);
const openThreads = computed(() => props.threads.filter((t) => !t.resolved));

async function toggleViewed(viewed: boolean) {
  isViewed.value = viewed;
  open.value = !viewed;
  viewedError.value = null;
  try {
    await api.reviewSetViewed(props.taskId, props.review.source, props.file.path, viewed);
  } catch (e) {
    isViewed.value = !viewed;
    open.value = viewed;
    viewedError.value = errorMessage(e);
  }
}

async function comment(
  body: string,
  target: CommentTarget,
  line: number,
  side: SplitSide,
  close: () => void,
) {
  await api.reviewComment({
    task_id: props.taskId,
    source: props.review.source,
    path: props.file.path,
    line,
    side: side === SplitSide.old ? 'old' : 'new',
    body,
    target,
  });
  close();
}
</script>

<template>
  <section :id="'file-' + file.path" class="file">
    <header class="file-head">
      <button class="link" @click="open = !open">{{ open ? '▾' : '▸' }} {{ file.path }}</button>
      <span class="add">+{{ file.additions }}</span>
      <span class="del">−{{ file.deletions }}</span>
      <span class="spacer" />
      <button v-if="openThreads.length" @click="emit('toAgent', openThreads)">→ Agent</button>
      <label>
        <input
          type="checkbox"
          :checked="isViewed"
          @change="toggleViewed(($event.target as HTMLInputElement).checked)"
        />
        Viewed
      </label>
    </header>
    <p v-if="viewedError" class="error">{{ viewedError }}</p>
    <!-- Rendered only while open: the library has no virtual scrolling. -->
    <DiffView
      v-if="open"
      :data="data"
      :extend-data="extend"
      :diff-view-mode="split ? DiffModeEnum.Split : DiffModeEnum.Unified"
      :diff-view-theme="activeTheme"
      :diff-view-highlight="true"
      :diff-view-add-widget="true"
    >
      <template #widget="{ lineNumber, side, onClose }">
        <CommentEditor
          :buttons="buttons"
          :submit="(body, target) => comment(body, target, lineNumber, side, onClose)"
          @cancel="onClose"
        />
      </template>
      <template #extend="{ data: lineThreads }">
        <ThreadView
          v-for="t in lineThreads as ReviewThread[]"
          :key="t.id"
          :task-id="taskId"
          :thread="t"
          :can-publish="review.source === 'pr' && review.reviews_supported"
          :selected="selected.has(t.id)"
          @select="(s) => emit('select', t.id, s)"
          @to-agent="(th) => emit('toAgent', [th])"
        />
      </template>
    </DiffView>
  </section>
</template>

<style scoped>
.file {
  border: 1px solid var(--border);
  border-radius: 6px;
  margin-bottom: 10px;
}
.file-head {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 6px 10px;
  position: sticky;
  top: 0;
  background: var(--bg);
  z-index: 1;
}
.file-head label {
  display: flex;
  gap: 6px;
  align-items: center;
}
.file-head input {
  width: auto;
}
.spacer {
  flex: 1;
}
.add {
  color: #2da44e;
}
.del {
  color: var(--danger);
}
.link {
  background: none;
  border: none;
  font-weight: 600;
}
</style>
