<script setup lang="ts">
import { ChevronDown, ChevronRight, File, Folder } from '@lucide/vue';
import { ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { openFile } from '../dock/main';
import type { DirEntry } from '../types';

const props = defineProps<{ taskId: number; entry: DirEntry; depth: number; reload: number }>();

const expanded = ref(false);
const children = ref<DirEntry[]>([]);
const error = ref('');

async function load() {
  try {
    children.value = await api.listDir(props.taskId, props.entry.path);
    error.value = '';
  } catch (e) {
    error.value = errorMessage(e);
  }
}

function click() {
  if (props.entry.kind === 'file') {
    void openFile(props.taskId, props.entry.path);
    return;
  }
  expanded.value = !expanded.value;
  if (expanded.value) void load();
}

watch(
  () => props.reload,
  () => {
    if (expanded.value) void load();
  },
);
</script>

<template>
  <button
    class="node"
    :class="{ ignored: entry.ignored, changed: entry.changed && entry.kind === 'file' }"
    :style="{ paddingLeft: `${8 + depth * 12}px` }"
    :title="entry.path"
    @click="click"
  >
    <template v-if="entry.kind === 'dir'">
      <ChevronDown v-if="expanded" /><ChevronRight v-else /><Folder />
    </template>
    <template v-else><span class="spacer" /><File /></template>
    <span class="name">{{ entry.name }}</span>
    <span v-if="entry.kind === 'dir' && entry.changed" class="dot" />
  </button>
  <template v-if="expanded">
    <p v-if="error" class="muted msg" :style="{ paddingLeft: `${20 + depth * 12}px` }">
      {{ error }}
    </p>
    <FileTreeNode
      v-for="child in children"
      :key="child.path"
      :task-id="taskId"
      :entry="child"
      :depth="depth + 1"
      :reload="reload"
    />
  </template>
</template>

<style scoped>
.node {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 4px;
  width: 100%;
  border: 0;
  background: transparent;
  color: var(--text);
  padding-top: 2px;
  padding-bottom: 2px;
  padding-right: 8px;
  text-align: left;
  border-radius: 0;
}
.node:hover {
  background: var(--select);
}
.node svg {
  width: 14px;
  height: 14px;
  flex: none;
}
.spacer {
  width: 14px;
  flex: none;
}
.name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ignored {
  opacity: 0.55;
}
.changed {
  color: var(--accent);
}
.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent);
  flex: none;
}
.msg {
  margin: 0;
  font-size: 12px;
}
</style>
