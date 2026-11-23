<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import {
  Button,
  Checkbox,
  Dialog,
  Field,
  SegmentedControl,
  Select,
  Tabs,
  Textarea,
  TextInput,
} from '@asterism-dev/lumen';
import { activityFor } from '../activity';
import { api, errorMessage, RpcError } from '../api';
import { baseChoice } from '../baseBranch';
import {
  defaultMachineFor,
  builtinAgents,
  LOCAL_MACHINE,
  machineAgents,
  machineOptions as machineOptionsFor,
  parseProviderValue,
  providerGroups,
  suggestVmName,
  vmOptionsDraft,
  vmOptionsPayload,
} from '../machines';
import type { SettingsDraft } from '../pluginsView';
import { leaveSettings } from '../settingsGuard';
import { matchAction } from '../shortcuts';
import { addSession, addTask, state, toast } from '../store';
import {
  createRequest,
  defaultBranch,
  finalSlug,
  liveSlug,
  randomSlug,
  randomSuffix,
} from '../taskForm';
import { issueToCreate } from '../taskSources';
import type {
  IssueDetails,
  MachineProviderInfo,
  PluginInfo,
  PrHit,
  ProjectBranches,
  ProviderMachineChoice,
  TaskIssue,
  TaskSourceInfo,
} from '../types';
import ActivitySteps from './ActivitySteps.vue';
import BasedOnPicker from './BasedOnPicker.vue';
import Kbd from './Kbd.vue';
import SettingField from './SettingField.vue';
import WorkspaceSettings from './WorkspaceSettings.vue';
import WorktreeConflict from './WorktreeConflict.vue';
import {
  detectConflict,
  keepChoice,
  reloadsOnError,
  replaceBlocked,
  type ConflictChoice,
} from '../worktreeConflict';

const props = defineProps<{ projectId: number }>();
const emit = defineEmits<{ close: [] }>();

const placeholder = randomSlug();
const suffix = randomSuffix();
const projectId = ref(props.projectId);
const projectOptions = computed(() =>
  state.projects
    .filter((p) => p.id === props.projectId || p.workspace_id === state.activeWorkspace)
    .map((p) => ({ value: p.id, label: p.name })),
);
const agentOptions = computed(() =>
  agents.value.map((a) => ({ value: a.name, label: a.display_name || a.name })),
);
const sessionOptions = computed(() => [
  { value: 'agent' as const, label: 'Agent', disabled: !agents.value.length },
  { value: 'shell' as const, label: 'Shell' },
]);
const DELETE_KEY = 'asterism.newVm.deleteWithTasks';
const providers = ref<MachineProviderInfo[]>([]);
const providersError = ref<string | null>(null);
const groups = computed(() => providerGroups(providers.value));
const machineOptions = computed(() => [
  ...machineOptionsFor(state.machines, true),
  ...(providersError.value
    ? [
        {
          value: 'provider:unavailable',
          label: `Providers unavailable: ${providersError.value}`,
          disabled: true,
        },
      ]
    : []),
  ...groups.value.flatMap((g) =>
    g.error
      ? [{ value: `provider:${g.provider}:error`, label: `${g.label}: ${g.error}`, disabled: true }]
      : g.options.map((o) => ({ ...o, label: `${g.label} · ${o.label}` })),
  ),
]);
const machineId = ref<number | string>(LOCAL_MACHINE);
const providerChoice = computed(() => parseProviderValue(machineId.value));
const newVm = computed(() => (providerChoice.value?.ref === null ? providerChoice.value : null));
const newVmInfo = computed(() => providers.value.find((p) => p.provider === newVm.value?.provider));
const vmName = ref('');
const vmNameTouched = ref(false);
const vmOptions = ref<SettingsDraft>({ values: {}, secrets: {}, cleared: [] });
const deleteWithTasks = ref(readDeleteWithTasks());
const takenNames = computed(() => [
  ...state.machines.map((m) => m.name),
  ...providers.value.flatMap((p) => p.vms.map((v) => v.name)),
]);
const plugins = ref<PluginInfo[]>([]);
const agents = computed(() =>
  providerChoice.value
    ? builtinAgents(state.agents, plugins.value)
    : machineAgents(state.machines.find((m) => m.id === machineId.value) ?? null, state.agents),
);

function readDeleteWithTasks(): boolean {
  try {
    return localStorage.getItem(DELETE_KEY) !== 'false';
  } catch {
    return true;
  }
}

function onDeleteWithTasks(value: boolean | 'indeterminate') {
  deleteWithTasks.value = value === true;
  try {
    localStorage.setItem(DELETE_KEY, String(deleteWithTasks.value));
  } catch {
    // Remembering the choice is optional.
  }
}

const TABS = [
  { value: 'conversation', label: 'Initial Conversation' },
  { value: 'workspace', label: 'Workspace Settings' },
];
const title = ref('');
const prompt = ref('');
const agent = ref(agents.value[0]?.name ?? '');
const session = ref<'agent' | 'shell'>(agents.value.length ? 'agent' : 'shell');
watch(
  agents,
  (list) => {
    if (!list.some((a) => a.name === agent.value)) agent.value = list[0]?.name ?? '';
    if (!list.length) session.value = 'shell';
  },
  { immediate: true },
);
api.plugins().then(
  (list) => {
    if (!closed) plugins.value = list;
  },
  () => {},
);
api.machineProviders().then(
  (list) => {
    if (!closed) providers.value = list;
  },
  (e) => {
    if (!closed) providersError.value = errorMessage(e);
  },
);
watch(newVmInfo, (info) => {
  vmOptions.value = vmOptionsDraft(info?.create_options ?? []);
  vmNameTouched.value = false;
});
watch(
  [() => title.value, takenNames, newVm],
  () => {
    if (newVm.value && !vmNameTouched.value)
      vmName.value = suggestVmName(finalSlug(title.value) || placeholder, takenNames.value);
  },
  { immediate: true },
);
const tab = ref('conversation');
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
const token = ref('');
const activity = computed(() => activityFor(state.activities, token.value));
let closed = false;
const titleInput = ref<InstanceType<typeof TextInput>>();

const branches = ref<ProjectBranches | null>(null);
const loadingBranches = ref(false);
const sources = ref<TaskSourceInfo[]>([]);
const conflict = computed(() =>
  detectConflict(branches.value, mode.value, branch.value, checkout.value),
);
const onConflict = ref<ConflictChoice>('reuse');
watch(conflict, (c, old) => {
  if (c) onConflict.value = c.name === old?.name ? keepChoice(c, onConflict.value) : 'reuse';
});
const replacing = computed(() => !!conflict.value && onConflict.value === 'replace');
const canCreate = computed(
  () =>
    (mode.value === 'checkout' ? !!checkout.value : baseChoice(branches.value).canCreate) &&
    !(replacing.value && conflict.value && replaceBlocked(conflict.value)),
);

async function loadBranches() {
  const id = projectId.value;
  loadingBranches.value = true;
  try {
    const result = await api.projectBranches(id);
    if (id !== projectId.value) return;
    branches.value = result;
    // A refresh keeps the chosen base while it still exists.
    if (![...result.local, ...result.remote].includes(base.value))
      base.value = baseChoice(result).selected;
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
  api.taskSources(id).then(
    (s) => {
      if (id === projectId.value) sources.value = s;
    },
    () => {
      if (id === projectId.value) sources.value = [];
    },
  );
  loadBranches();
}
watch(
  projectId,
  () => {
    error.value = null;
    issue.value = null;
    prBranch.value = null;
    mode.value = 'new';
    checkout.value = '';
    clearPrefill();
    machineId.value = defaultMachineFor(
      state.projects.find((p) => p.id === projectId.value),
      state.machines,
    );
    loadProject();
  },
  { immediate: true },
);

function onTitle(e: Event) {
  const el = e.target as HTMLInputElement;
  const raw = el.value;
  const slug = liveSlug(raw);
  const pos = Math.min(
    Math.max(slug.length - (raw.length - (el.selectionStart ?? raw.length)), 0),
    slug.length,
  );
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

// On window, not the form: WebKit doesn't focus clicked buttons, so focus often sits on body.
function onKey(e: KeyboardEvent) {
  if (matchAction(e) !== 'confirm') return;
  e.preventDefault();
  submit();
}

onMounted(() => {
  titleInput.value?.$el.focus();
  window.addEventListener('keydown', onKey);
});
onUnmounted(() => {
  closed = true;
  window.removeEventListener('keydown', onKey);
});

function providerMachine(): ProviderMachineChoice | undefined {
  const choice = providerChoice.value;
  if (!choice) return undefined;
  if (choice.ref !== null) return { provider: choice.provider, kind: 'existing', ref: choice.ref };
  return {
    provider: choice.provider,
    kind: 'new',
    name: vmName.value.trim(),
    options: vmOptionsPayload(vmOptions.value),
    delete_with_tasks: deleteWithTasks.value,
  };
}

async function submit() {
  if (busy.value || loadingBranches.value || !canCreate.value) return;
  if (newVm.value && !vmName.value.trim()) {
    error.value = 'Enter a name for the new VM.';
    return;
  }
  if (mode.value === 'checkout' && !checkout.value) {
    error.value = 'Choose a branch to check out.';
    tab.value = 'workspace';
    return;
  }
  busy.value = true;
  token.value = crypto.randomUUID();
  const sent = conflict.value ? onConflict.value : null;
  try {
    if (!(await leaveSettings())) return;
    const created = await api.createTask({
      ...createRequest({
        projectId: projectId.value,
        title: title.value,
        placeholder,
        prompt: prompt.value,
        agent: session.value === 'agent' ? agent.value : '',
        mode: mode.value,
        base: base.value,
        branch: branch.value,
        checkout: checkout.value,
        push: push.value,
        issue: kind.value === 'issue' ? issue.value : null,
        onConflict: sent,
        machineId: typeof machineId.value === 'number' ? machineId.value || null : null,
      }),
      provider_machine: providerMachine(),
      activity_token: token.value,
    });
    if (closed) return;
    addTask(state, created.task);
    state.projectPage = null;
    state.selectedTaskId = created.task.id;
    delete state.collapsed[projectId.value];
    if (created.session) addSession(state, created.session);
    if (created.warning) toast(created.warning);
    emit('close');
  } catch (e) {
    if (closed) return;
    error.value = errorMessage(e);
    // The conflict appeared after the branches loaded; reload so the notice replaces the error.
    if (e instanceof RpcError && reloadsOnError(e.kind, sent)) {
      await loadBranches();
      if (conflict.value) error.value = null;
    }
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <Dialog :open="true" width="520px" closable @update:open="(o) => !o && emit('close')">
    <template #title>
      Create Task in
      <Select v-model="projectId" :options="projectOptions" class="project" aria-label="Project" />
      <template v-if="state.machines.length || groups.length || providersError">
        on
        <Select
          v-model="machineId"
          :options="machineOptions"
          class="project"
          aria-label="Machine"
        />
      </template>
    </template>
    <form class="create-task" @submit.prevent="submit">
      <TextInput
        ref="titleInput"
        :model-value="title"
        class="title"
        :placeholder="placeholder"
        aria-label="Task name"
        spellcheck="false"
        @input="onTitle"
      />

      <div v-if="newVm" class="new-vm">
        <Field label="VM name">
          <TextInput
            v-model="vmName"
            spellcheck="false"
            @update:model-value="vmNameTouched = true"
          />
        </Field>
        <SettingField
          v-for="spec in newVmInfo?.create_options ?? []"
          :key="spec.key"
          :spec="spec"
          :draft="vmOptions"
        />
        <Checkbox :model-value="deleteWithTasks" @update:model-value="onDeleteWithTasks">
          Delete VM with its last task
        </Checkbox>
      </div>

      <BasedOnPicker
        v-model:kind="kind"
        :project-id="projectId"
        :sources="sources"
        @issue="onIssue"
        @pr="onPr"
        @open-settings="openSettings"
      />

      <Tabs v-model="tab" :tabs="TABS" keep-mounted>
        <template #conversation>
          <div class="conversation tab-panel">
            <SegmentedControl
              v-model="session"
              :options="sessionOptions"
              size="sm"
              class="session"
              aria-label="Session"
            />
            <template v-if="session === 'agent'">
              <Select v-model="agent" :options="agentOptions" aria-label="Agent" />
              <p v-if="providerChoice" class="muted">
                The agent must be installed in the VM's image.
              </p>
              <Textarea
                v-model="prompt"
                :rows="4"
                placeholder="Describe what the agent should do…"
              />
            </template>
            <p v-else class="muted">Opens a shell in the worktree — no agent is started.</p>
          </div>
        </template>
        <template #workspace>
          <WorkspaceSettings
            v-model:mode="mode"
            v-model:base="base"
            v-model:branch="branch"
            v-model:checkout="checkout"
            v-model:push="push"
            class="tab-panel"
            :branches="branches"
            :loading="loadingBranches"
            :pr-branch="prBranch"
            @touch-branch="branchTouched = true"
            @refresh="loadBranches"
          />
        </template>
      </Tabs>

      <WorktreeConflict
        v-if="conflict"
        v-model="onConflict"
        :conflict="conflict"
        @deleted="loadBranches"
      />
      <ActivitySteps v-if="activity && (busy || error)" :steps="activity.steps" />
      <p v-if="error" class="error">{{ error }}</p>
      <footer class="actions">
        <Button
          type="submit"
          :variant="replacing ? 'danger' : 'accent'"
          :disabled="busy || loadingBranches || !canCreate"
        >
          {{ replacing ? 'Replace & create' : 'Create' }} <Kbd action="confirm" />
        </Button>
      </footer>
    </form>
  </Dialog>
</template>

<style scoped>
.create-task {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.project {
  width: auto;
  border: none;
  font-weight: 600;
  padding: 0 4px;
}
.title {
  font-size: 18px;
  border: none;
  padding: 6px 4px;
}
.new-vm {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.conversation {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.tab-panel {
  padding-top: 10px;
}
.session {
  align-self: flex-start;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
