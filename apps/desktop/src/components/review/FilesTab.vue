<script setup lang="ts">
import { Button, Checkbox } from '@asterism-dev/lumen';
import { computed, ref } from 'vue';
import { splitPatch, threadsByFile } from '../../review';
import type { ReviewResult, ReviewThread } from '../../types';
import FileDiff from './FileDiff.vue';
import FileTree from './FileTree.vue';

const props = defineProps<{
  taskId: number;
  review: ReviewResult;
  patch: string;
  commit: string | null | undefined;
  selected: Set<string>;
}>();
const emit = defineEmits<{
  select: [id: string, on: boolean];
  toAgent: [threads: ReviewThread[]];
  sendSelection: [];
  showAll: [];
}>();
const split = ref(false);
const readOnly = computed(() => props.commit !== undefined);
const files = computed(() => splitPatch(props.patch));
const byFile = computed(() =>
  readOnly.value ? new Map<string, ReviewThread[]>() : threadsByFile(props.review.threads),
);
const counts = computed(() => new Map([...byFile.value].map(([p, t]) => [p, t.length])));
const viewed = computed(() => new Set(readOnly.value ? [] : props.review.viewed_files));
const openCount = computed(() => props.review.threads.filter((t) => !t.resolved).length);

function open(path: string) {
  document.getElementById('file-' + path)?.scrollIntoView({ block: 'start' });
}
</script>

<template>
  <div class="files-tab">
    <div v-if="commit !== undefined" class="banner">
      {{ commit === null ? 'Uncommitted changes' : `Commit ${commit.slice(0, 7)}` }} ·
      <Button variant="ghost" size="sm" class="link" @click="emit('showAll')"
        >Show all changes</Button
      >
    </div>
    <div class="bar">
      <Checkbox v-model="split">Side by side</Checkbox>
      <span class="spacer" />
      <Button
        v-if="!readOnly"
        size="sm"
        :disabled="!review.threads.length"
        @click="emit('sendSelection')"
      >
        Send to agent ({{ selected.size || openCount }})
      </Button>
    </div>
    <p v-if="!files.length" class="muted message">No changes.</p>
    <div v-else class="body">
      <FileTree
        :files="files"
        :viewed="viewed"
        :counts="counts"
        :hide-progress="readOnly"
        @open="open"
      />
      <div class="files">
        <FileDiff
          v-for="(f, i) in files"
          :key="f.path + (viewed.has(f.path) ? ':v' : '')"
          :task-id="taskId"
          :file="f"
          :index="i"
          :review="review"
          :threads="byFile.get(f.path) ?? []"
          :split="split"
          :viewed="viewed.has(f.path)"
          :selected="selected"
          :read-only="readOnly"
          @select="(id, on) => emit('select', id, on)"
          @to-agent="(t) => emit('toAgent', t)"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.files-tab {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.banner {
  padding: 6px 14px;
  background: var(--select);
  font-size: 12px;
}
.link {
  color: var(--accent);
}
.bar {
  display: flex;
  gap: 10px;
  align-items: center;
  padding: 6px 14px;
}
.spacer {
  flex: 1;
}
.message {
  padding: 0 14px;
}
.body {
  flex: 1;
  min-height: 0;
  display: flex;
}
.files {
  flex: 1;
  min-width: 0;
  overflow: auto;
  padding: 8px 14px;
}
</style>
