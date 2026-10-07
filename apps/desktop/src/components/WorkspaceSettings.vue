<script setup lang="ts">
import { computed, ref } from 'vue';
import { ChevronDown, FolderGit2, GitBranch } from '@lucide/vue';
import { baseChoice } from '../baseBranch';
import { checkoutChoices, worktreeDir } from '../taskForm';
import type { ProjectBranches } from '../types';

const props = defineProps<{ branches: ProjectBranches | null; loading: boolean; prBranch: string | null }>();
const mode = defineModel<'new' | 'checkout'>('mode', { required: true });
const base = defineModel<string>('base', { required: true });
const branch = defineModel<string>('branch', { required: true });
const checkout = defineModel<string>('checkout', { required: true });
const push = defineModel<boolean>('push', { required: true });
const emit = defineEmits<{ touchBranch: [] }>();

const expanded = ref(false);
const choice = computed(() => baseChoice(props.branches));
const choices = computed(() => checkoutChoices(props.branches?.branches ?? []));
const path = computed(() => {
  const dir = worktreeDir(mode.value === 'new' ? branch.value.trim() : checkout.value);
  const root = props.branches?.worktree_root;
  return root ? `${root}/${dir}` : dir;
});
</script>

<template>
  <div class="workspace">
    <div class="bar">
      <button type="button" class="disclosure" :aria-expanded="expanded" @click="expanded = !expanded">
        Settings <ChevronDown :size="12" :class="{ flipped: expanded }" />
      </button>
      <div class="segmented small" role="group" aria-label="Branch">
        <button type="button" :disabled="!!prBranch" :class="{ active: mode === 'checkout' }" :aria-pressed="mode === 'checkout'" @click="mode = 'checkout'">Checkout branch</button>
        <button type="button" :disabled="!!prBranch" :class="{ active: mode === 'new' }" :aria-pressed="mode === 'new'" @click="mode = 'new'">Create new branch</button>
      </div>
    </div>

    <template v-if="expanded">
      <template v-if="mode === 'new'">
        <label class="field">
          <span class="muted">From branch</span>
          <span class="with-icon"><GitBranch :size="14" />
            <select v-model="base" :disabled="loading || !choice.canCreate">
              <option v-if="loading" value="">Fetching…</option>
              <option v-for="b in choice.options" :key="b" :value="b">{{ b }}</option>
            </select>
          </span>
        </label>
        <p v-if="choice.hint" class="muted">{{ choice.hint }}</p>
        <div class="field">
          <label><span class="muted">Branch name</span>
            <input v-model="branch" class="mono" spellcheck="false" @input="emit('touchBranch')" />
          </label>
          <label class="switch"><input v-model="push" type="checkbox" role="switch" /> Push branch to remote</label>
        </div>
      </template>
      <label v-else class="field">
        <span class="muted">Branch</span>
        <span class="with-icon"><GitBranch :size="14" />
          <select v-model="checkout" :disabled="!!prBranch || loading">
            <option v-if="prBranch" :value="prBranch">{{ prBranch }}</option>
            <option v-else value="" disabled>Choose a branch</option>
            <template v-if="!prBranch">
              <option v-for="b in choices" :key="b" :value="b">{{ b }}</option>
            </template>
          </select>
        </span>
      </label>
    </template>

    <p class="muted path"><FolderGit2 :size="14" /> Worktree: <span class="mono">{{ path }}</span></p>
  </div>
</template>

<style scoped>
.workspace { display: flex; flex-direction: column; gap: 8px; }
.bar { display: flex; align-items: center; gap: 8px; }
.disclosure { flex: 1; display: inline-flex; align-items: center; justify-content: space-between; text-align: left; }
.flipped { transform: rotate(180deg); }
.segmented.small button { border: none; padding: 2px 8px; }
.field { border: 1px solid var(--border); border-radius: 8px; padding: 6px 8px; display: flex; flex-direction: column; gap: 6px; }
.field input:not([type='checkbox']), .field select { border: none; padding: 2px 0; }
.with-icon { display: flex; align-items: center; gap: 6px; }
.switch { flex-direction: row !important; align-items: center; gap: 8px !important; }
.switch input { width: auto; }
.path { display: flex; align-items: center; gap: 6px; font-size: 12px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
</style>
