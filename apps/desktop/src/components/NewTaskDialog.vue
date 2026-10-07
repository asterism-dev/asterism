<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { X } from '@lucide/vue';
import { api, errorMessage } from '../api';
import { baseChoice } from '../baseBranch';
import { leaveSettings } from '../settingsGuard';
import { addSession, addTask, state, toast } from '../store';
import { createRequest, defaultBranch, finalSlug, liveSlug, randomSlug, randomSuffix } from '../taskForm';
import { issueToCreate } from '../taskSources';
import type { IssueDetails, PrHit, ProjectBranches, TaskIssue, TaskSourceInfo } from '../types';
import BasedOnPicker from './BasedOnPicker.vue';
import WorkspaceSettings from './WorkspaceSettings.vue';

const props = defineProps<{ projectId: number }>();
const emit = defineEmits<{ close: [] }>();

const placeholder = randomSlug();
const suffix = randomSuffix();
const agents = computed(() => state.agents.filter((a) => a.available));
const projectId = ref(props.projectId);
const title = ref('');
const prompt = ref('');
const agent = ref(agents.value[0]?.name ?? '');
const session = ref<'agent' | 'shell'>(agents.value.length ? 'agent' : 'shell');
const tab = ref<'conversation' | 'workspace'>('conversation');
const kind = ref<'issue' | 'pr'>('issue');
const issue = ref<TaskIssue | null>(null);
const prBranch = ref<string | null>(null);
const prefilled = ref(false);
const promptPrefilled = ref(false);
const mode = ref<'new' | 'checkout'>('new');
const base = ref('');
const branch = ref(defaultBranch(placeholder, suffix));
const branchTouched = ref(false);
const checkout = ref('');
const push = ref(true);
const error = ref<string | null>(null);
const busy = ref(false);
const titleInput = ref<HTMLInputElement>();
const picker = ref<InstanceType<typeof BasedOnPicker>>();

const branches = ref<ProjectBranches | null>(null);
const loadingBranches = ref(false);
const sources = ref<TaskSourceInfo[]>([]);
const canCreate = computed(() => mode.value === 'checkout' ? !!checkout.value : baseChoice(branches.value).canCreate);

async function loadBranches() {
  const id = projectId.value;
  loadingBranches.value = true;
  try {
    const result = await api.projectBranches(id);
    if (id !== projectId.value) return;
    branches.value = result;
    // A refresh keeps the chosen base while it still exists.
    if (![...result.local, ...result.remote].includes(base.value)) base.value = baseChoice(result).selected;
  } catch (e) {
    if (id === projectId.value) error.value = errorMessage(e);
  } finally {
    if (id === projectId.value) loadingBranches.value = false;
  }
}

function loadProject() {
  const id = projectId.value;
  branches.value = null;
  base.value = '';
  api.taskSources(id).then((s) => { if (id === projectId.value) sources.value = s; }, () => { if (id === projectId.value) sources.value = []; });
  loadBranches();
}
watch(projectId, () => {
  error.value = null;
  issue.value = null;
  prBranch.value = null;
  mode.value = 'new';
  checkout.value = '';
  clearPrefill();
  loadProject();
}, { immediate: true });

function onTitle(e: Event) {
  const el = e.target as HTMLInputElement;
  const raw = el.value;
  const slug = liveSlug(raw);
  const pos = Math.min(Math.max(slug.length - (raw.length - (el.selectionStart ?? raw.length)), 0), slug.length);
  title.value = slug;
  el.value = slug;
  el.setSelectionRange(pos, pos);
}
watch(title, (t) => {
  if (!branchTouched.value) branch.value = defaultBranch(finalSlug(t) || placeholder, suffix);
});

function clearPrefill() {
  // Title and prompt came from the pick; keep them only for that pick.
  if (prefilled.value) {
    title.value = '';
    if (promptPrefilled.value) prompt.value = '';
    branchTouched.value = false;
    branch.value = defaultBranch(placeholder, suffix);
  }
  prefilled.value = false;
  promptPrefilled.value = false;
}

function onIssue(d: IssueDetails | null) {
  clearPrefill();
  issue.value = d ? issueToCreate(d) : null;
  if (!d) return;
  title.value = finalSlug(d.name);
  prompt.value = d.prompt;
  promptPrefilled.value = true;
  if (d.branch) {
    branch.value = d.branch;
    branchTouched.value = true;
  }
  prefilled.value = true;
  error.value = null;
}

function onPr(hit: PrHit | null) {
  clearPrefill();
  prBranch.value = hit?.head_branch ?? null;
  mode.value = hit ? 'checkout' : 'new';
  checkout.value = hit?.head_branch ?? '';
  if (!hit) return;
  title.value = finalSlug(hit.title);
  prefilled.value = true;
  error.value = null;
}

function openSettings(plugin: string) {
  state.pluginSettingsRequest = plugin;
  state.settingsOpen = true;
  emit('close');
}

function onEsc() {
  if (!picker.value?.closePanel()) emit('close');
}

// On window, not the form: WebKit doesn't focus clicked buttons, so focus often sits on body.
function onKey(e: KeyboardEvent) {
  if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    submit();
  } else if (e.key === 'Escape') {
    onEsc();
  }
}

onMounted(() => {
  titleInput.value?.focus();
  window.addEventListener('keydown', onKey);
});
onUnmounted(() => window.removeEventListener('keydown', onKey));

async function submit() {
  if (busy.value || loadingBranches.value || !canCreate.value) return;
  if (mode.value === 'checkout' && !checkout.value) {
    error.value = 'Choose a branch to check out.';
    tab.value = 'workspace';
    return;
  }
  busy.value = true;
  try {
    if (!(await leaveSettings())) return;
    const created = await api.createTask(createRequest({
      projectId: projectId.value, title: title.value, placeholder, prompt: prompt.value,
      agent: session.value === 'agent' ? agent.value : '', mode: mode.value, base: base.value, branch: branch.value,
      checkout: checkout.value, push: push.value, issue: kind.value === 'issue' ? issue.value : null,
    }));
    addTask(state, created.task);
    state.projectPage = null;
    state.selectedTaskId = created.task.id;
    delete state.collapsed[projectId.value];
    if (created.session) addSession(state, created.session);
    if (created.warning) toast(created.warning);
    emit('close');
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('close')">
    <form class="modal create-task" @submit.prevent="submit">
      <header>
        <h2>Create Task in
          <select v-model="projectId" class="project" aria-label="Project">
            <option v-for="p in state.projects" :key="p.id" :value="p.id">{{ p.name }}</option>
          </select>
        </h2>
        <button type="button" class="icon" aria-label="Close" @click="emit('close')"><X :size="16" /></button>
      </header>

      <input ref="titleInput" :value="title" class="title" :placeholder="placeholder" aria-label="Task name" spellcheck="false" @input="onTitle" />

      <BasedOnPicker ref="picker" v-model:kind="kind" :project-id="projectId" :sources="sources"
        @issue="onIssue" @pr="onPr" @open-settings="openSettings" />

      <div class="tabs" role="tablist">
        <button type="button" role="tab" :aria-selected="tab === 'conversation'" :class="{ active: tab === 'conversation' }" @click="tab = 'conversation'">Initial Conversation</button>
        <button type="button" role="tab" :aria-selected="tab === 'workspace'" :class="{ active: tab === 'workspace' }" @click="tab = 'workspace'">Workspace Settings</button>
      </div>

      <div v-show="tab === 'conversation'" class="conversation">
        <div class="segmented small" role="group" aria-label="Session">
          <button type="button" :disabled="!agents.length" :class="{ active: session === 'agent' }" :aria-pressed="session === 'agent'" @click="session = 'agent'">Agent</button>
          <button type="button" :class="{ active: session === 'shell' }" :aria-pressed="session === 'shell'" @click="session = 'shell'">Shell</button>
        </div>
        <template v-if="session === 'agent'">
          <select v-model="agent" aria-label="Agent">
            <option v-for="a in agents" :key="a.name" :value="a.name">{{ a.display_name || a.name }}</option>
          </select>
          <textarea v-model="prompt" rows="4" placeholder="Describe what the agent should do…" />
        </template>
        <p v-else class="muted">Opens a shell in the worktree — no agent is started.</p>
      </div>

      <WorkspaceSettings v-show="tab === 'workspace'" v-model:mode="mode" v-model:base="base" v-model:branch="branch"
        v-model:checkout="checkout" v-model:push="push" :branches="branches" :loading="loadingBranches" :pr-branch="prBranch"
        @touch-branch="branchTouched = true" @refresh="loadBranches" />

      <p v-if="error" class="error">{{ error }}</p>
      <footer class="actions">
        <button type="submit" class="primary" :disabled="busy || loadingBranches || !canCreate">Create <kbd>⌘</kbd><kbd>↵</kbd></button>
      </footer>
    </form>
  </div>
</template>

<style scoped>
.create-task { width: 520px; }
header { display: flex; align-items: center; justify-content: space-between; }
header h2 { margin: 0; font-size: 14px; display: flex; align-items: center; gap: 6px; }
.project { width: auto; border: none; font-weight: 600; padding: 0 4px; }
.icon { border: none; background: none; display: inline-flex; align-items: center; padding: 4px; }
.title { font-size: 18px; border: none; padding: 6px 4px; }
.tabs { display: flex; gap: 4px; }
.tabs button { border: none; background: none; color: var(--muted); }
.tabs button.active { color: inherit; background: var(--select); }
.conversation { display: flex; flex-direction: column; gap: 8px; }
.conversation .segmented { align-self: flex-start; }
.segmented.small button { border: none; padding: 2px 8px; }
.primary { background: #1f8f4e; border-color: #1f8f4e; color: #fff; display: inline-flex; align-items: center; gap: 4px; }
.primary:hover:not(:disabled) { background: #187a42; }
.primary:disabled { opacity: 0.6; }
kbd { font: inherit; font-size: 11px; padding: 0 4px; border-radius: 3px; background: rgba(255, 255, 255, 0.2); }
</style>
