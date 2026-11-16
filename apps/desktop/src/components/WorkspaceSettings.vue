<script setup lang="ts">
import { SegmentedControl, Switch, TextInput } from '@asterism-dev/lumen';
import { computed } from 'vue';
import { FolderGit2 } from '@lucide/vue';
import { baseChoice } from '../baseBranch';
import type { ProjectBranches } from '../types';
import BranchPicker from './BranchPicker.vue';

const props = defineProps<{
  branches: ProjectBranches | null;
  loading: boolean;
  prBranch: string | null;
}>();
const mode = defineModel<'new' | 'checkout'>('mode', { required: true });
const base = defineModel<string>('base', { required: true });
const branch = defineModel<string>('branch', { required: true });
const checkout = defineModel<string>('checkout', { required: true });
const push = defineModel<boolean>('push', { required: true });
const emit = defineEmits<{ touchBranch: []; refresh: [] }>();

const modes = [
  { value: 'checkout' as const, label: 'Checkout branch' },
  { value: 'new' as const, label: 'Create new branch' },
];
const choice = computed(() => baseChoice(props.branches));
const local = computed(() => props.branches?.local ?? []);
const remote = computed(() => props.branches?.remote ?? []);
const path = computed(() => {
  const dir = mode.value === 'new' ? branch.value.trim() : checkout.value;
  const root = props.branches?.worktree_root;
  return root ? `${root}/${dir}` : dir;
});
</script>

<template>
  <div class="workspace">
    <SegmentedControl v-model="mode" :options="modes" :disabled="!!prBranch" aria-label="Branch" />

    <template v-if="mode === 'new'">
      <BranchPicker
        v-model="base"
        label="From branch"
        :local="local"
        :remote="remote"
        :loading="loading"
        :disabled="!loading && !choice.canCreate"
        @refresh="emit('refresh')"
      />
      <p v-if="choice.hint" class="muted">{{ choice.hint }}</p>
      <div class="field">
        <label
          ><span class="muted">Branch name</span>
          <TextInput
            v-model="branch"
            class="mono"
            spellcheck="false"
            @input="emit('touchBranch')"
          />
        </label>
        <Switch v-model="push">Push branch to remote</Switch>
      </div>
    </template>
    <BranchPicker
      v-else
      v-model="checkout"
      label="Branch"
      :local="local"
      :remote="remote"
      strip-remote
      :loading="loading"
      :disabled="!!prBranch"
      @refresh="emit('refresh')"
    />

    <p class="muted path">
      <FolderGit2 :size="14" /> Worktree: <span class="mono">{{ path }}</span>
    </p>
  </div>
</template>

<style scoped>
.workspace {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.field {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 6px 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.field input:not([type='checkbox']) {
  border: none;
  padding: 2px 0;
}
/* The global .modal label rule stacks label content vertically */
.lm-choice {
  flex-direction: row !important;
  align-items: center;
  gap: 8px !important;
}
.path {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
</style>
