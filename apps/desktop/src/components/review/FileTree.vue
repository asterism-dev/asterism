<script setup lang="ts">
import {
  ChevronDown,
  ChevronRight,
  FileDiff,
  FileMinus2,
  FilePlus2,
  FileSymlink,
  Folder,
} from '@lucide/vue';
import { computed, ref } from 'vue';
import { buildTree, filterFiles, flattenTree, type FileDiff as Diff } from '../../review';

const props = defineProps<{ files: Diff[]; viewed: Set<string>; counts: Map<string, number> }>();
const emit = defineEmits<{ open: [path: string] }>();
const query = ref('');
const collapsed = ref(new Set<string>());
const rows = computed(() =>
  flattenTree(buildTree(filterFiles(props.files, query.value)), collapsed.value),
);
const ICONS = { added: FilePlus2, modified: FileDiff, deleted: FileMinus2, renamed: FileSymlink };

function toggle(path: string) {
  const next = new Set(collapsed.value);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  collapsed.value = next;
}
</script>

<template>
  <div class="tree">
    <input v-model="query" class="filter" placeholder="Filter files…" />
    <div class="muted progress">
      {{ files.filter((f) => viewed.has(f.path)).length }} / {{ files.length }} viewed
    </div>
    <div
      v-for="{ node, depth } in rows"
      :key="node.kind + node.path"
      class="row"
      :class="{ done: node.kind === 'file' && viewed.has(node.path) }"
      :style="{ paddingLeft: 6 + depth * 14 + 'px' }"
      :title="node.path"
      @click="node.kind === 'dir' ? toggle(node.path) : emit('open', node.path)"
    >
      <template v-if="node.kind === 'dir'">
        <component :is="collapsed.has(node.path) ? ChevronRight : ChevronDown" :size="14" />
        <Folder :size="14" />
        <span class="name">{{ node.name }}</span>
      </template>
      <template v-else>
        <component :is="ICONS[node.file.status]" :size="14" :class="node.file.status" />
        <span class="name">{{ node.name }}</span>
        <span v-if="counts.get(node.path)" class="count">{{ counts.get(node.path) }}</span>
        <span v-if="viewed.has(node.path)" class="check">✓</span>
      </template>
    </div>
  </div>
</template>

<style scoped>
.tree {
  width: 260px;
  flex: none;
  overflow: auto;
  padding: 8px 6px;
  border-right: 1px solid var(--border);
  font-size: 12px;
}
.filter {
  width: 100%;
  margin-bottom: 6px;
}
.progress {
  padding: 0 6px 6px;
}
.row {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 3px 6px;
  border-radius: 4px;
  cursor: pointer;
  white-space: nowrap;
}
.row:hover {
  background: var(--select);
}
.row.done {
  opacity: 0.55;
}
.name {
  overflow: hidden;
  text-overflow: ellipsis;
}
.count {
  margin-left: auto;
  font-size: 11px;
  color: var(--muted);
}
.check {
  color: #2da44e;
}
.added {
  color: #2da44e;
}
.deleted {
  color: var(--danger);
}
.renamed,
.modified {
  color: var(--accent);
}
</style>
