<script setup lang="ts">
import { ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { TREE_WIDTH, setTreeWidth, treeWidth } from '../fileTreeBar';
import { state } from '../store';
import type { DirEntry } from '../types';
import FileTreeNode from './FileTreeNode.vue';
import PaneHandle from './PaneHandle.vue';

const props = defineProps<{ taskId: number }>();

const root = ref<DirEntry[]>([]);
const error = ref('');
const reload = ref(0);

watch(
  [() => props.taskId, () => state.reviewVersion[props.taskId]],
  ([id], [prevId]) => {
    if (id !== prevId) root.value = [];
    reload.value++;
    api
      .listDir(id, '')
      .then((entries) => {
        if (props.taskId !== id) return;
        root.value = entries;
        error.value = '';
      })
      .catch((e) => {
        if (props.taskId === id) error.value = errorMessage(e);
      });
  },
  { immediate: true },
);
</script>

<template>
  <PaneHandle
    side="right"
    :width="treeWidth"
    @resize="setTreeWidth"
    @reset="setTreeWidth(TREE_WIDTH.initial)"
  />
  <aside class="file-tree" :style="{ width: `${treeWidth}px` }">
    <p v-if="error" class="muted msg">{{ error }}</p>
    <FileTreeNode
      v-for="entry in root"
      :key="entry.path"
      :task-id="taskId"
      :entry="entry"
      :depth="0"
      :reload="reload"
    />
  </aside>
</template>

<style scoped>
.file-tree {
  flex: none;
  overflow: auto;
  padding: 6px 0;
  border-left: 1px solid var(--border);
  background: var(--panel);
  font-size: 13px;
}
.msg {
  margin: 0;
  padding: 0 10px;
  font-size: 12px;
}
</style>
