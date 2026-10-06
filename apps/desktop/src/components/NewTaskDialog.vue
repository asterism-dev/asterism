<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { api, errorMessage, RpcError } from '../api';
import { leaveSettings } from '../settingsGuard';
import { addSession, addTask, state } from '../store';
import { createTitle, issueToCreate, searchParams, sourceOptions } from '../taskSources';
import type { IssueDetails, IssueHit, TaskSourceInfo } from '../types';

const props = defineProps<{ projectId: number }>();
const emit = defineEmits<{ close: [] }>();

const agents = computed(() => state.agents.filter((a) => a.available));
const projectId = ref(props.projectId);
const title = ref('');
const prompt = ref('');
const agent = ref(agents.value[0]?.name ?? '');
const error = ref<string | null>(null);
const busy = ref(false);
const titleInput = ref<HTMLInputElement>();

const sources = ref<TaskSourceInfo[]>([]);
const options = computed(() => sourceOptions(sources.value));
const source = ref('');
const query = ref('');
const hits = ref<IssueHit[]>([]);
const searchError = ref<{ message: string; plugin: string | null; setup: boolean } | null>(null);
const searching = ref(false);
const details = ref<IssueDetails | null>(null);
let searchSeq = 0;
let pickSeq = 0;
let sourcesSeq = 0;
let debounce: ReturnType<typeof setTimeout> | undefined;

async function loadSources() {
  const seq = ++sourcesSeq;
  let loaded: TaskSourceInfo[];
  try {
    loaded = await api.taskSources(projectId.value);
  } catch {
    loaded = [];
  }
  if (seq !== sourcesSeq) return;
  sources.value = loaded;
  if (source.value && !options.value.some((o) => o.id === source.value && !o.disabled)) source.value = '';
}

function searchFailure(e: unknown) {
  const plugin = options.value.find((o) => o.id === source.value)?.plugin ?? null;
  return { message: errorMessage(e), plugin, setup: e instanceof RpcError && e.kind === 'needs_setup' };
}

async function search() {
  if (!source.value) return;
  const seq = ++searchSeq;
  searching.value = true;
  const { query: q, assigned_to_me } = searchParams(query.value);
  try {
    const result = await api.searchIssues(projectId.value, source.value, q, assigned_to_me);
    if (seq === searchSeq) { hits.value = result; searchError.value = null; }
  } catch (e) {
    if (seq !== searchSeq) return;
    hits.value = [];
    searchError.value = searchFailure(e);
  } finally {
    if (seq === searchSeq) searching.value = false;
  }
}

async function pick(hit: IssueHit) {
  const seq = ++pickSeq;
  try {
    const d = await api.getIssue(projectId.value, source.value, hit.key);
    if (seq !== pickSeq) return;
    details.value = d;
    title.value = d.name;
    prompt.value = d.prompt;
    error.value = null;
    searchError.value = null;
  } catch (e) {
    if (seq !== pickSeq) return;
    const failure = searchFailure(e);
    if (failure.setup) searchError.value = failure;
    else error.value = failure.message;
  }
}

function openSettings(plugin: string) {
  state.pluginSettingsRequest = plugin;
  state.settingsOpen = true;
  emit('close');
}

watch(projectId, () => {
  searchSeq++;
  pickSeq++;
  details.value = null;
  hits.value = [];
  searchError.value = null;
  loadSources().then(search);
});
watch(source, () => {
  clearTimeout(debounce);
  pickSeq++;
  details.value = null;
  hits.value = [];
  searchError.value = null;
  search();
});
watch(query, () => { clearTimeout(debounce); debounce = setTimeout(search, 250); });
onMounted(() => {
  titleInput.value?.focus();
  loadSources();
});
onUnmounted(() => clearTimeout(debounce));

async function submit() {
  if (source.value && details.value?.source !== source.value) {
    error.value = 'Pick an issue.';
    return;
  }
  if (!details.value && !title.value.trim()) {
    error.value = 'Give the task a title.';
    return;
  }
  busy.value = true;
  try {
    if (!(await leaveSettings())) return;
    const created = await api.createTask({
      project_id: projectId.value,
      title: createTitle(title.value, details.value),
      prompt: (agent.value && prompt.value.trim()) || null,
      agent: agent.value || null,
      issue: details.value && source.value ? issueToCreate(details.value) : null,
    });
    addTask(state, created.task);
    state.projectPage = null;
    state.selectedTaskId = created.task.id;
    delete state.collapsed[projectId.value];
    if (created.session) addSession(state, created.session);
    emit('close');
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('close')" @keydown.esc="emit('close')">
    <form class="modal" @submit.prevent="submit">
      <h2>New task</h2>
      <label>Project
        <select v-model="projectId">
          <option v-for="p in state.projects" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
      </label>
      <div v-if="sources.length" class="segmented" role="group" aria-label="Source">
        <button v-for="o in options" :key="o.id" type="button" :disabled="o.disabled" :title="o.hint ?? ''"
          :aria-pressed="source === o.id" :class="{ active: source === o.id }" @click="source = o.id">{{ o.label }}</button>
      </div>
      <template v-if="source">
        <label>Search <input v-model="query" @keydown.enter.prevent placeholder="Empty shows your open issues" /></label>
        <p v-if="searchError" class="error">
          {{ searchError.message }}
          <button v-if="searchError.setup && searchError.plugin" type="button" @click="openSettings(searchError.plugin)">Open settings</button>
        </p>
        <ul v-else class="issue-list">
          <li v-for="h in hits" :key="h.key" :class="{ selected: details?.key === h.key }">
            <button type="button" class="issue-row" @click="pick(h)">
              <span class="mono">{{ h.key }}</span> <span class="name">{{ h.title }}</span>
              <span class="muted">{{ h.state }}<template v-if="h.assignee"> · {{ h.assignee }}</template></span>
            </button>
          </li>
          <li v-if="!hits.length && !searching" class="muted">No issues.</li>
        </ul>
        <p v-if="details?.branch" class="muted">Branch <span class="mono">{{ details.branch }}</span></p>
      </template>
      <label>Title <input ref="titleInput" v-model="title" placeholder="Fix the login redirect" /></label>
      <label>Prompt <textarea
        v-model="prompt"
        rows="5"
        :disabled="!agent"
        :placeholder="agent ? 'Optional — sent to the agent on start' : 'Choose an agent to send a prompt'"
      /></label>
      <label>Agent
        <select v-model="agent">
          <option v-for="a in agents" :key="a.name" :value="a.name">{{ a.display_name || a.name }}</option>
          <option value="">No agent (empty worktree)</option>
        </select>
      </label>
      <p v-if="error" class="error">{{ error }}</p>
      <div class="actions">
        <button type="button" @click="emit('close')">Cancel</button>
        <button type="submit" :disabled="busy">Create</button>
      </div>
    </form>
  </div>
</template>

<style scoped>
.issue-list { list-style: none; margin: 0; padding: 0; max-height: 220px; overflow-y: auto; border: 1px solid var(--border); border-radius: 6px; }
.issue-list li { display: flex; }
.issue-row { all: unset; box-sizing: border-box; flex: 1; display: flex; gap: 8px; align-items: baseline; padding: 4px 8px; cursor: pointer; min-width: 0; }
.issue-row:focus-visible { outline: 2px solid var(--accent, currentColor); outline-offset: -2px; }
.issue-list li .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.issue-list li:hover, .issue-list li.selected { background: var(--select); }
</style>
