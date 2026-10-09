<script setup lang="ts">
import { FileDiff, GitCommitHorizontal, ListChecks, MessageSquare } from '@lucide/vue';
import { computed } from 'vue';
import { diffBar, type ReviewTab } from '../../review';

const props = defineProps<{
  tabs: { id: ReviewTab; label: string; count: number }[];
  active: ReviewTab;
  additions: number;
  deletions: number;
}>();
const emit = defineEmits<{ select: [id: ReviewTab] }>();
const ICONS = {
  conversation: MessageSquare,
  commits: GitCommitHorizontal,
  checks: ListChecks,
  files: FileDiff,
};
const bar = computed(() => diffBar(props.additions, props.deletions));
</script>

<template>
  <nav class="tabs" role="tablist">
    <button
      v-for="t in tabs"
      :key="t.id"
      role="tab"
      :aria-selected="t.id === active"
      :class="{ active: t.id === active }"
      @click="emit('select', t.id)"
    >
      <component :is="ICONS[t.id]" :size="15" />
      {{ t.label }}
      <span class="pill">{{ t.count }}</span>
    </button>
    <span class="spacer" />
    <span class="stat">
      <span class="add">+{{ additions.toLocaleString() }}</span>
      <span class="del">−{{ deletions.toLocaleString() }}</span>
      <span v-for="(s, i) in bar" :key="i" class="sq" :class="s" />
    </span>
  </nav>
</template>

<style scoped>
.tabs {
  display: flex;
  align-items: flex-end;
  gap: 2px;
  padding: 0 14px;
  border-bottom: 1px solid var(--border);
}
.tabs button {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  background: none;
  border: 1px solid transparent;
  border-bottom: none;
  border-radius: 6px 6px 0 0;
  margin-bottom: -1px;
  color: var(--muted);
  cursor: pointer;
}
.tabs button.active {
  color: var(--text);
  background: var(--panel);
  border-color: var(--border);
}
.pill {
  font-size: 11px;
  padding: 0 7px;
  border-radius: 10px;
  background: var(--select);
  color: var(--text);
}
.spacer {
  flex: 1;
}
.stat {
  display: flex;
  align-items: center;
  gap: 4px;
  padding-bottom: 8px;
  font-size: 12px;
  font-weight: 600;
}
.add {
  color: #2da44e;
}
.del {
  color: var(--danger);
  margin-right: 4px;
}
.sq {
  width: 8px;
  height: 8px;
  border-radius: 1px;
  background: var(--border);
}
.sq.add {
  background: #2da44e;
}
.sq.del {
  background: var(--danger);
}
</style>
