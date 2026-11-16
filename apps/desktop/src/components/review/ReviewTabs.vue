<script setup lang="ts">
import { Badge, Tabs } from '@asterism-dev/lumen';
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
const items = computed(() => props.tabs.map((t) => ({ value: t.id, label: t.label })));
const counts = computed(() => Object.fromEntries(props.tabs.map((t) => [t.id, t.count])));
const bar = computed(() => diffBar(props.additions, props.deletions));
</script>

<template>
  <Tabs
    :model-value="active"
    :tabs="items"
    variant="underline"
    class="review-tabs"
    @update:model-value="(v) => emit('select', v as ReviewTab)"
  >
    <template #tab="{ tab }">
      <component :is="ICONS[tab.value as ReviewTab]" :size="15" />
      {{ tab.label }}
      <Badge variant="pill" weight="normal">{{ counts[tab.value] }}</Badge>
    </template>
    <template #end>
      <span class="stat">
        <span class="add">+{{ additions.toLocaleString() }}</span>
        <span class="del">−{{ deletions.toLocaleString() }}</span>
        <span v-for="(s, i) in bar" :key="i" class="sq" :class="s" />
      </span>
    </template>
  </Tabs>
</template>

<style scoped>
.review-tabs {
  flex: none;
}
.review-tabs :deep(.lm-tabs__bar) {
  padding: 0 4px 0 14px;
}
.stat {
  display: flex;
  align-items: center;
  gap: 4px;
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
