<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref } from 'vue';
import { Check, ChevronDown, GitBranch, RefreshCw } from '@lucide/vue';
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

const root = ref<HTMLElement>();
const searchInput = ref<HTMLInputElement>();
const open = ref(false);
const tab = ref<'local' | 'remote'>('local');
const query = ref('');

const valueOf = (entry: string) => (props.stripRemote ? entry.replace(/^origin\//, '') : entry);
const shown = computed(() => filterBranches(tab.value === 'local' ? props.local : props.remote, query.value));

function onOutside(e: MouseEvent) {
  if (!root.value?.contains(e.target as Node)) close();
}

function close() {
  open.value = false;
  document.removeEventListener('mousedown', onOutside);
}

function toggle() {
  if (open.value) return close();
  query.value = '';
  tab.value = props.local.includes(model.value) ? 'local' : props.remote.some((r) => valueOf(r) === model.value) ? 'remote' : 'local';
  open.value = true;
  document.addEventListener('mousedown', onOutside);
  nextTick(() => searchInput.value?.focus());
}

function pick(entry: string) {
  model.value = valueOf(entry);
  close();
}

onUnmounted(() => document.removeEventListener('mousedown', onOutside));
</script>

<template>
  <div ref="root" class="branch-picker">
    <button type="button" class="field" :disabled="disabled" :aria-expanded="open" @click="toggle">
      <span class="muted label">{{ label }}</span>
      <span class="value">
        <GitBranch :size="14" />
        <span class="mono name">{{ loading && !model ? 'Fetching…' : model || 'Choose a branch' }}</span>
        <ChevronDown :size="14" class="muted" />
      </span>
    </button>

    <div v-if="open" class="popover" @keydown.esc.stop="close">
      <div class="segmented small tabs" role="group" aria-label="Branch kind">
        <button type="button" :class="{ active: tab === 'local' }" :aria-pressed="tab === 'local'" @click="tab = 'local'">
          Local <span class="count">{{ local.length }}</span>
        </button>
        <button type="button" :class="{ active: tab === 'remote' }" :aria-pressed="tab === 'remote'" @click="tab = 'remote'">
          Remote <span class="count">{{ remote.length }}</span>
        </button>
      </div>
      <div class="search">
        <input ref="searchInput" v-model="query" placeholder="Search branches" spellcheck="false" @keydown.enter.prevent />
        <button type="button" class="icon" :class="{ spinning: loading }" aria-label="Refresh branches" title="Refresh branches" @click="emit('refresh')">
          <RefreshCw :size="14" />
        </button>
      </div>
      <ul class="list">
        <li v-for="b in shown" :key="b">
          <button type="button" class="row" @click="pick(b)">
            <span class="mono name">{{ b }}</span>
            <Check v-if="valueOf(b) === model" :size="14" />
          </button>
        </li>
        <li v-if="!shown.length" class="muted none">No branches.</li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.branch-picker { position: relative; }
.field { all: unset; box-sizing: border-box; width: 100%; display: flex; flex-direction: column; gap: 4px; border: 1px solid var(--border); border-radius: 8px; padding: 6px 8px; cursor: pointer; }
.field:disabled { cursor: default; opacity: 0.6; }
.field:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.label { font-size: 12px; }
.value { display: flex; align-items: center; gap: 6px; min-width: 0; }
.name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.popover { position: absolute; left: 0; right: 0; top: calc(100% + 4px); z-index: 3; background: var(--panel); border: 1px solid var(--border); border-radius: 8px; padding: 6px; display: flex; flex-direction: column; gap: 6px; box-shadow: 0 6px 24px rgba(0, 0, 0, 0.18); }
.tabs { display: flex; }
.tabs button { flex: 1; border: none; padding: 2px 8px; }
.count { color: var(--muted); font-size: 11px; margin-left: 4px; }
.search { display: flex; align-items: center; gap: 6px; border-bottom: 1px solid var(--border); padding-bottom: 4px; }
.search input { border: none; padding: 2px 0; }
.icon { border: none; background: none; display: inline-flex; padding: 4px; }
.spinning :deep(svg) { animation: spin 1s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
.list { list-style: none; margin: 0; padding: 0; max-height: 240px; overflow-y: auto; }
.list li { display: flex; }
.row { all: unset; box-sizing: border-box; flex: 1; display: flex; align-items: center; gap: 8px; padding: 4px 6px; border-radius: 4px; cursor: pointer; min-width: 0; }
.row:hover, .row:focus-visible { background: var(--select); }
.none { padding: 4px 6px; }
</style>
