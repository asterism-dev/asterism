<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { ChevronDown, GitPullRequest, GitPullRequestDraft, X } from '@lucide/vue';
import {
  Button,
  Combobox,
  Menu,
  Select,
  SegmentedControl,
  type ComboboxOption,
  type MenuItem,
} from '@asterism-dev/lumen';
import { api, errorMessage, RpcError } from '../api';
import { issueStateClass, prDisabledReason } from '../taskForm';
import { searchParams } from '../taskSources';
import type { IssueDetails, IssueHit, PrHit, PrListState, TaskSourceInfo } from '../types';
import ProviderIcon from './ProviderIcon.vue';

const props = defineProps<{ projectId: number; sources: TaskSourceInfo[] }>();
const kind = defineModel<'issue' | 'pr'>('kind', { required: true });
const emit = defineEmits<{
  issue: [IssueDetails | null];
  pr: [PrHit | null];
  openSettings: [plugin: string];
}>();

const open = ref(false);
const query = ref('');
const source = ref('');
const prState = ref<PrListState>('open');
const issueHits = ref<IssueHit[]>([]);
const prHits = ref<PrHit[]>([]);
const repo = ref<string | null>(null);
const forge = ref('');
const picked = ref<{ value: string; key: string; label: string } | null>(null);
const failure = ref<{ message: string; plugin: string | null; setup: boolean } | null>(null);
const searching = ref(false);
let seq = 0;
let fetched: IssueDetails | null = null;
let debounce: ReturnType<typeof setTimeout> | undefined;

const KINDS = [
  { value: 'issue' as const, label: 'Issue' },
  { value: 'pr' as const, label: 'Pull Request' },
];
const PR_STATES = [
  { value: 'open' as const, label: 'Open' },
  { value: 'closed' as const, label: 'Closed' },
];

const sourceInfo = computed(() => props.sources.find((s) => s.id === source.value) ?? null);
const items = computed<ComboboxOption[]>(() =>
  kind.value === 'pr'
    ? prHits.value.map((h) => ({
        value: String(h.number),
        label: h.title,
        disabled: !!prDisabledReason(h),
        title: prDisabledReason(h) ?? undefined,
      }))
    : issueHits.value.map((h) => ({ value: h.key, label: h.title })),
);
const issueByKey = computed(() => Object.fromEntries(issueHits.value.map((h) => [h.key, h])));
const prByValue = computed(() =>
  Object.fromEntries(prHits.value.map((h) => [String(h.number), h])),
);
const providerItems = computed<MenuItem[]>(() =>
  props.sources.map((s) => ({
    label: s.display_name,
    icon: ProviderIcon,
    iconProps: { id: s.id, size: 14 },
    title: s.reason ?? undefined,
    disabled: !s.available,
    checked: s.id === source.value,
    action: () => (source.value = s.id),
  })),
);

watch(
  () => props.sources,
  (list) => {
    if (!list.some((s) => s.id === source.value && s.available))
      source.value = list.find((s) => s.available)?.id ?? '';
  },
  { immediate: true },
);

async function search() {
  const mine = ++seq;
  searching.value = true;
  failure.value = null;
  try {
    if (kind.value === 'pr') {
      const result = await api.searchPullRequests(
        props.projectId,
        query.value.trim(),
        prState.value,
      );
      if (mine !== seq) return;
      prHits.value = result.hits;
      repo.value = result.repo;
      forge.value = result.forge;
    } else if (source.value) {
      const p = searchParams(query.value);
      const hits = await api.searchIssues(props.projectId, source.value, p.query, p.assigned_to_me);
      if (mine === seq) issueHits.value = hits;
    } else {
      issueHits.value = [];
    }
  } catch (e) {
    if (mine !== seq) return;
    issueHits.value = [];
    prHits.value = [];
    const plugin = kind.value === 'issue' ? (sourceInfo.value?.plugin ?? null) : null;
    failure.value = {
      message: errorMessage(e),
      plugin,
      setup: e instanceof RpcError && e.kind === 'needs_setup',
    };
  } finally {
    if (mine === seq) searching.value = false;
  }
}

function reset() {
  seq++;
  clearTimeout(debounce);
  picked.value = null;
  open.value = false;
  query.value = '';
  issueHits.value = [];
  prHits.value = [];
  failure.value = null;
}

function clear() {
  if (kind.value === 'pr') emit('pr', null);
  else emit('issue', null);
  picked.value = null;
}

async function fetchIssue(key: string, signal: AbortSignal) {
  const details = await api.getIssue(props.projectId, source.value, key);
  if (!signal.aborted) fetched = details;
}

function onPick(value: string) {
  if (kind.value === 'issue') {
    if (!fetched) return;
    picked.value = { value, key: fetched.key, label: fetched.title };
    emit('issue', fetched);
  } else {
    const hit = prByValue.value[value];
    picked.value = { value, key: `#${hit.number}`, label: hit.title };
    emit('pr', hit);
  }
}

watch(kind, (now, before) => {
  if (picked.value) {
    if (before === 'pr') emit('pr', null);
    else emit('issue', null);
  }
  reset();
  // Prefetch PRs so the placeholder can name the repository.
  if (now === 'pr') search();
});
watch(
  () => props.projectId,
  () => {
    reset();
    repo.value = null;
    if (kind.value === 'pr') search();
  },
);
watch(open, (isOpen) => {
  if (isOpen) search();
});
watch([source, prState], () => {
  if (open.value) search();
});
watch(query, () => {
  clearTimeout(debounce);
  debounce = setTimeout(search, 250);
});
onUnmounted(() => clearTimeout(debounce));
</script>

<template>
  <div class="card">
    <div class="head">
      <span>Based on</span>
      <SegmentedControl v-model="kind" :options="KINDS" size="sm" aria-label="Based on" />
    </div>
    <div class="pick">
      <Combobox
        v-model:search="query"
        v-model:open="open"
        :model-value="picked?.value ?? ''"
        :items="items"
        :loading="searching"
        :search-placeholder="
          kind === 'pr'
            ? 'Search pull requests…'
            : `Search ${sourceInfo?.display_name ?? 'issues'}…`
        "
        :before-select="kind === 'issue' ? fetchIssue : undefined"
        @update:model-value="onPick"
      >
        <template #trigger>
          <button type="button" class="trigger" :class="{ placeholder: !picked }">
            <template v-if="picked">
              <span class="mono muted">{{ picked.key }}</span>
              <span class="name">{{ picked.label }}</span>
            </template>
            <template v-else-if="kind === 'issue'">Select issue</template>
            <template v-else-if="repo"
              >Select a PR from <ProviderIcon :id="forge" /> {{ repo }}</template
            >
            <template v-else>Select pull request</template>
          </button>
        </template>
        <template #search-actions>
          <Menu v-if="kind === 'issue' && sources.length" :items="providerItems">
            <Button variant="ghost" size="sm" aria-label="Issue provider">
              <ProviderIcon :id="source" /> <ChevronDown :size="12" />
            </Button>
          </Menu>
          <Select
            v-if="kind === 'pr'"
            v-model="prState"
            :options="PR_STATES"
            class="state"
            aria-label="Pull request state"
          />
        </template>
        <template #item="{ item }">
          <template v-if="kind === 'issue'">
            <span class="dot" :class="issueStateClass(issueByKey[item.value].state)" />
            <span class="name">{{ item.label }}</span>
            <span class="mono muted">{{ item.value }}</span>
          </template>
          <template v-else>
            <component
              :is="prByValue[item.value].draft ? GitPullRequestDraft : GitPullRequest"
              :size="14"
              :class="prByValue[item.value].draft ? 'muted' : 'open-pr'"
            />
            <span class="lines">
              <span class="name">{{ item.label }}</span>
              <span class="muted sub"
                >{{ prByValue[item.value].author }} ·
                <span class="mono">{{ prByValue[item.value].head_branch }}</span></span
              >
            </span>
            <span class="mono muted">#{{ item.value }}</span>
          </template>
        </template>
        <template #empty>
          <span v-if="failure" class="error">
            {{ failure.message }}
            <Button
              v-if="failure.setup && failure.plugin"
              @click="emit('openSettings', failure.plugin)"
            >
              Open settings
            </Button>
          </span>
          <template v-else-if="searching">Loading…</template>
          <template v-else>{{ kind === 'pr' ? 'No pull requests.' : 'No issues.' }}</template>
        </template>
      </Combobox>
      <Button v-if="picked" variant="ghost" size="sm" :icon="X" aria-label="Clear" @click="clear" />
    </div>
  </div>
</template>

<style scoped>
.card {
  border: 1px solid var(--border);
  border-radius: 8px;
}
.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 8px;
  border-bottom: 1px solid var(--border);
}
.pick {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-right: 8px;
}
.trigger {
  all: unset;
  box-sizing: border-box;
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 8px;
  cursor: pointer;
}
.trigger.placeholder {
  justify-content: center;
  padding: 18px 8px;
  color: var(--muted);
}
.trigger:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}
.name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.state {
  width: auto;
  border: none;
}
.lines {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.sub {
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex: none;
  border: 1.5px solid var(--muted);
}
.dot.progress {
  border-color: #d6a400;
  background: linear-gradient(90deg, #d6a400 50%, transparent 50%);
}
.dot.done {
  border-color: #2e9d5b;
  background: #2e9d5b;
}
.dot.canceled {
  border-color: var(--muted);
  background: var(--muted);
}
.open-pr {
  color: #2e9d5b;
}
</style>
