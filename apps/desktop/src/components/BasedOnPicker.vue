<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { Check, ChevronDown, GitPullRequest, GitPullRequestDraft, X } from '@lucide/vue';
import { api, errorMessage, RpcError } from '../api';
import { issueStateClass, prDisabledReason } from '../taskForm';
import { searchParams } from '../taskSources';
import type { IssueDetails, IssueHit, PrHit, PrListState, TaskSourceInfo } from '../types';
import ProviderIcon from './ProviderIcon.vue';

const props = defineProps<{ projectId: number; sources: TaskSourceInfo[] }>();
const kind = defineModel<'issue' | 'pr'>('kind', { required: true });
const emit = defineEmits<{ issue: [IssueDetails | null]; pr: [PrHit | null]; openSettings: [plugin: string] }>();

const open = ref(false);
const menuOpen = ref(false);
const query = ref('');
const source = ref('');
const prState = ref<PrListState>('open');
const issueHits = ref<IssueHit[]>([]);
const prHits = ref<PrHit[]>([]);
const repo = ref<string | null>(null);
const forge = ref('');
const picked = ref<{ key: string; label: string } | null>(null);
const failure = ref<{ message: string; plugin: string | null; setup: boolean } | null>(null);
const searching = ref(false);
let seq = 0;
let debounce: ReturnType<typeof setTimeout> | undefined;

const sourceInfo = computed(() => props.sources.find((s) => s.id === source.value) ?? null);
const empty = computed(() => !searching.value && (kind.value === 'pr' ? !prHits.value.length : !issueHits.value.length));

watch(() => props.sources, (list) => {
  if (!list.some((s) => s.id === source.value && s.available)) source.value = list.find((s) => s.available)?.id ?? '';
}, { immediate: true });

async function search() {
  const mine = ++seq;
  searching.value = true;
  failure.value = null;
  try {
    if (kind.value === 'pr') {
      const result = await api.searchPullRequests(props.projectId, query.value.trim(), prState.value);
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
    const plugin = kind.value === 'issue' ? sourceInfo.value?.plugin ?? null : null;
    failure.value = { message: errorMessage(e), plugin, setup: e instanceof RpcError && e.kind === 'needs_setup' };
  } finally {
    if (mine === seq) searching.value = false;
  }
}

function reset() {
  seq++;
  clearTimeout(debounce);
  picked.value = null;
  open.value = false;
  menuOpen.value = false;
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

async function pickIssue(hit: IssueHit) {
  const mine = ++seq;
  try {
    const details = await api.getIssue(props.projectId, source.value, hit.key);
    if (mine !== seq) return;
    picked.value = { key: details.key, label: details.title };
    open.value = false;
    emit('issue', details);
  } catch (e) {
    if (mine === seq) failure.value = { message: errorMessage(e), plugin: sourceInfo.value?.plugin ?? null, setup: e instanceof RpcError && e.kind === 'needs_setup' };
  }
}

function pickPr(hit: PrHit) {
  if (prDisabledReason(hit)) return;
  picked.value = { key: `#${hit.number}`, label: hit.title };
  open.value = false;
  emit('pr', hit);
}

function toggle() {
  open.value = !open.value;
  if (open.value) search();
}

function closePanel(): boolean {
  if (!open.value) return false;
  open.value = false;
  menuOpen.value = false;
  return true;
}
defineExpose({ closePanel });

watch(kind, (now, before) => {
  if (picked.value) {
    if (before === 'pr') emit('pr', null);
    else emit('issue', null);
  }
  reset();
  // Prefetch PRs so the placeholder can name the repository.
  if (now === 'pr') search();
});
watch(() => props.projectId, () => {
  reset();
  repo.value = null;
  if (kind.value === 'pr') search();
});
watch([source, prState], () => { if (open.value) search(); });
watch(query, () => { clearTimeout(debounce); debounce = setTimeout(search, 250); });
onUnmounted(() => clearTimeout(debounce));
</script>

<template>
  <div class="based-on">
    <div class="card">
      <div class="head">
        <span>Based on</span>
        <div class="segmented small" role="group" aria-label="Based on">
          <button type="button" :class="{ active: kind === 'issue' }" :aria-pressed="kind === 'issue'" @click="kind = 'issue'">Issue</button>
          <button type="button" :class="{ active: kind === 'pr' }" :aria-pressed="kind === 'pr'" @click="kind = 'pr'">Pull Request</button>
        </div>
      </div>
      <div v-if="picked" class="picked">
        <span class="mono muted">{{ picked.key }}</span>
        <span class="name">{{ picked.label }}</span>
        <button type="button" class="icon" aria-label="Clear" @click="clear"><X :size="14" /></button>
      </div>
      <button v-else type="button" class="placeholder" @click="toggle">
        <template v-if="kind === 'issue'">Select issue</template>
        <template v-else-if="repo">Select a PR from <ProviderIcon :id="forge" /> {{ repo }}</template>
        <template v-else>Select pull request</template>
      </button>
    </div>

    <div v-if="open" class="panel" @keydown.esc.stop="closePanel">
      <div class="search">
        <div v-if="kind === 'issue' && sources.length" class="provider">
          <button type="button" class="icon" :aria-expanded="menuOpen" aria-label="Issue provider" @click="menuOpen = !menuOpen">
            <ProviderIcon :id="source" /> <ChevronDown :size="12" />
          </button>
          <ul v-if="menuOpen" class="menu">
            <li v-for="s in sources" :key="s.id">
              <button type="button" :disabled="!s.available" :title="s.reason ?? ''" @click="source = s.id; menuOpen = false">
                <ProviderIcon :id="s.id" /> <span class="name">{{ s.display_name }}</span>
                <Check v-if="s.id === source" :size="14" />
              </button>
            </li>
          </ul>
        </div>
        <input v-model="query" autofocus @keydown.enter.prevent
          :placeholder="kind === 'pr' ? 'Search pull requests…' : `Search ${sourceInfo?.display_name ?? 'issues'}…`" />
        <select v-if="kind === 'pr'" v-model="prState" class="state" aria-label="Pull request state">
          <option value="open">Open</option>
          <option value="closed">Closed</option>
        </select>
      </div>
      <p v-if="failure" class="error">
        {{ failure.message }}
        <button v-if="failure.setup && failure.plugin" type="button" @click="emit('openSettings', failure.plugin)">Open settings</button>
      </p>
      <ul v-else class="hits">
        <template v-if="kind === 'issue'">
          <li v-for="h in issueHits" :key="h.key">
            <button type="button" class="row" @click="pickIssue(h)">
              <span class="dot" :class="issueStateClass(h.state)" />
              <span class="name">{{ h.title }}</span>
              <span class="mono muted">{{ h.key }}</span>
            </button>
          </li>
        </template>
        <template v-else>
          <li v-for="h in prHits" :key="h.number">
            <button type="button" class="row two" :disabled="!!prDisabledReason(h)" :title="prDisabledReason(h) ?? ''" @click="pickPr(h)">
              <component :is="h.draft ? GitPullRequestDraft : GitPullRequest" :size="14" :class="h.draft ? 'muted' : 'open-pr'" />
              <span class="lines">
                <span class="name">{{ h.title }}</span>
                <span class="muted sub">{{ h.author }} · <span class="mono">{{ h.head_branch }}</span></span>
              </span>
              <span class="mono muted">#{{ h.number }}</span>
            </button>
          </li>
        </template>
        <li v-if="empty" class="muted none">{{ kind === 'pr' ? 'No pull requests.' : 'No issues.' }}</li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.based-on { display: flex; flex-direction: column; gap: 6px; }
.card { border: 1px solid var(--border); border-radius: 8px; }
.head { display: flex; align-items: center; justify-content: space-between; padding: 6px 8px; border-bottom: 1px solid var(--border); }
.segmented.small button { border: none; padding: 2px 8px; }
.placeholder { all: unset; box-sizing: border-box; width: 100%; padding: 18px 8px; text-align: center; color: var(--muted); cursor: pointer; }
.placeholder:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.picked { display: flex; align-items: center; gap: 8px; padding: 10px 8px; }
.picked .name, .row .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.icon { border: none; background: none; display: inline-flex; align-items: center; gap: 2px; padding: 2px 4px; }
.panel { border: 1px solid var(--border); border-radius: 8px; }
.search { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--border); position: relative; }
.search input { border: none; padding: 2px 0; }
.search .state { width: auto; border: none; }
.provider { position: relative; }
.menu { position: absolute; top: 100%; left: 0; z-index: 2; list-style: none; margin: 4px 0 0; padding: 4px; min-width: 160px; background: var(--panel); border: 1px solid var(--border); border-radius: 8px; }
.menu button { all: unset; box-sizing: border-box; width: 100%; display: flex; align-items: center; gap: 8px; padding: 4px 6px; border-radius: 4px; cursor: pointer; }
.menu button:hover:not(:disabled) { background: var(--select); }
.menu button:disabled { opacity: 0.5; cursor: default; }
.hits { list-style: none; margin: 0; padding: 4px 0; max-height: 260px; overflow-y: auto; }
.hits li { display: flex; }
.row { all: unset; box-sizing: border-box; flex: 1; display: flex; align-items: center; gap: 8px; padding: 4px 8px; cursor: pointer; min-width: 0; }
.row:hover:not(:disabled) { background: var(--select); }
.row:disabled { opacity: 0.5; cursor: default; }
.row:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.row.two { align-items: flex-start; }
.lines { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.sub { font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.none { padding: 4px 8px; }
.dot { width: 8px; height: 8px; border-radius: 50%; flex: none; border: 1.5px solid var(--muted); }
.dot.progress { border-color: #d6a400; background: linear-gradient(90deg, #d6a400 50%, transparent 50%); }
.dot.done { border-color: #2e9d5b; background: #2e9d5b; }
.dot.canceled { border-color: var(--muted); background: var(--muted); }
.open-pr { color: #2e9d5b; }
</style>
