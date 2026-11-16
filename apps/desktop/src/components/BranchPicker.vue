<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { GitBranch, RefreshCw } from '@lucide/vue';
import { Button, Combobox, SegmentedControl, type SegmentOption } from '@asterism-dev/lumen';
import { filterBranches } from '../taskForm';

const props = defineProps<{
  label: string;
  local: string[];
  remote: string[];
  disabled?: boolean;
  loading?: boolean;
  /** Remote entries select the branch name without `origin/`, as checkout expects. */
  stripRemote?: boolean;
}>();
const model = defineModel<string>({ required: true });
const emit = defineEmits<{ refresh: [] }>();

const open = ref(false);
const tab = ref<'local' | 'remote'>('local');
const query = ref('');

const valueOf = (entry: string) => (props.stripRemote ? entry.replace(/^origin\//, '') : entry);
const items = computed(() =>
  filterBranches(tab.value === 'local' ? props.local : props.remote, query.value).map((b) => ({
    value: valueOf(b),
    label: b,
  })),
);
const tabs = computed<SegmentOption<'local' | 'remote'>[]>(() => [
  { value: 'local', label: `Local ${props.local.length}` },
  { value: 'remote', label: `Remote ${props.remote.length}` },
]);

watch(open, (isOpen) => {
  if (!isOpen) return;
  tab.value = props.local.includes(model.value)
    ? 'local'
    : props.remote.some((r) => valueOf(r) === model.value)
      ? 'remote'
      : 'local';
});
</script>

<template>
  <Combobox
    v-model="model"
    v-model:search="query"
    v-model:open="open"
    :items="items"
    :label="label"
    :disabled="disabled"
    :loading="loading"
    search-placeholder="Search branches"
  >
    <template #value="{ value }">
      <GitBranch :size="14" />
      <span class="mono name">{{
        loading && !value ? 'Fetching…' : value || 'Choose a branch'
      }}</span>
    </template>
    <template #header>
      <SegmentedControl v-model="tab" :options="tabs" class="tabs" aria-label="Branch kind" />
    </template>
    <template #search-actions>
      <Button
        variant="ghost"
        size="sm"
        :icon="RefreshCw"
        :class="{ spinning: loading }"
        aria-label="Refresh branches"
        title="Refresh branches"
        @click="emit('refresh')"
      />
    </template>
    <template #item="{ item }">
      <span class="mono name">{{ item.label }}</span>
    </template>
    <template #empty>No branches.</template>
  </Combobox>
</template>

<style scoped>
.name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tabs :deep(button) {
  flex: 1;
  border: none;
  padding: 2px 8px;
}
.spinning :deep(svg) {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
