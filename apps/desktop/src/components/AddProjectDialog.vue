<script setup lang="ts">
import { open } from '@tauri-apps/plugin-dialog';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { activityFor } from '../activity';
import { api, errorMessage } from '../api';
import type { Project } from '../types';
import { matchAction } from '../shortcuts';
import { state, type ProjectDialogTab } from '../store';
import ActivitySteps from './ActivitySteps.vue';
import CloneForm from './CloneForm.vue';
import CreateRepoForm from './CreateRepoForm.vue';
import Kbd from './Kbd.vue';

const TABS: { id: ProjectDialogTab; label: string }[] = [
  { id: 'folder', label: 'Add folder' },
  { id: 'clone', label: 'Clone repository' },
  { id: 'create', label: 'New repository' },
];

const workspaceId = ref<number | null>(state.activeWorkspace);
const existing = ref<Project | null>(null);
const existingIn = computed(
  () =>
    state.workspaces.find((w) => w.id === existing.value?.workspace_id)?.name ??
    'another workspace',
);
const chosenName = computed(() => state.workspaces.find((w) => w.id === workspaceId.value)?.name);
const busy = ref(false);
const token = ref('');
const activity = computed(() => activityFor(state.activities, token.value));
const error = ref<string | null>(null);
const chooseButton = ref<HTMLButtonElement>();
const dialog = ref<HTMLElement>();
let closed = false;

// On window, like NewTaskDialog: WebKit doesn't focus clicked buttons, so focus often sits on body.
function onKey(e: KeyboardEvent) {
  if (matchAction(e) !== 'confirm' || busy.value) return;
  e.preventDefault();
  if (existing.value) return;
  if (state.projectDialog === 'folder') void chooseFolder();
  // Clicking (not requestSubmit) respects the button's disabled state and works on Safari 15.
  else dialog.value?.querySelector<HTMLButtonElement>('button[type="submit"]')?.click();
}

onMounted(() => {
  chooseButton.value?.focus();
  window.addEventListener('keydown', onKey);
});
onUnmounted(() => {
  closed = true;
  window.removeEventListener('keydown', onKey);
});

function close() {
  state.projectDialog = null;
}

function select(tab: ProjectDialogTab) {
  if (busy.value) return;
  existing.value = null;
  error.value = null;
  state.projectDialog = tab;
}

async function resolveExisting(action: 'move' | 'clone') {
  if (!existing.value || workspaceId.value === null) return;
  busy.value = true;
  error.value = null;
  try {
    token.value = crypto.randomUUID();
    const project =
      action === 'move'
        ? await api.moveProject(existing.value.id, workspaceId.value)
        : await api.cloneCopy(existing.value.id, workspaceId.value, token.value);
    if (closed) return;
    delete state.collapsed[project.id];
    state.projectDialog = null;
  } catch (e) {
    if (!closed) error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}

async function chooseFolder() {
  error.value = null;
  const path = await open({ directory: true, multiple: false });
  if (typeof path !== 'string') return;
  busy.value = true;
  try {
    const project = await api.addProject(path, workspaceId.value ?? undefined);
    delete state.collapsed[project.id];
    // The daemon returns an already-known project unchanged, so a mismatch means it lives elsewhere.
    if (workspaceId.value !== null && project.workspace_id !== workspaceId.value)
      existing.value = project;
    else state.projectDialog = null;
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="modal-backdrop" tabindex="-1" @click.self="close" @keydown.esc="close">
    <div ref="dialog" class="modal" role="dialog" aria-modal="true" aria-label="Add project">
      <label class="add-form"
        >Workspace
        <select v-model="workspaceId" :disabled="busy">
          <option v-for="w in state.workspaces" :key="w.id" :value="w.id">{{ w.name }}</option>
        </select>
      </label>
      <div class="segmented" role="tablist">
        <button
          v-for="t in TABS"
          :key="t.id"
          type="button"
          role="tab"
          :aria-selected="state.projectDialog === t.id"
          :class="{ active: state.projectDialog === t.id }"
          :disabled="busy && state.projectDialog !== t.id"
          @click="select(t.id)"
        >
          {{ t.label }}
        </button>
      </div>
      <div v-if="state.projectDialog === 'folder' && existing" class="add-form">
        <p>{{ existing.name }} is already in {{ existingIn }}.</p>
        <ActivitySteps v-if="activity && (busy || error)" :steps="activity.steps" />
        <p v-if="error" class="error">{{ error }}</p>
        <div class="actions">
          <button type="button" :disabled="busy" @click="close">Cancel</button>
          <button type="button" :disabled="busy" @click="resolveExisting('clone')">
            Create a second clone
          </button>
          <button type="button" class="primary" :disabled="busy" @click="resolveExisting('move')">
            Move to {{ chosenName }}
          </button>
        </div>
      </div>
      <div v-else-if="state.projectDialog === 'folder'" class="add-form">
        <p class="muted">Add a git repository that already exists on this computer.</p>
        <p v-if="error" class="error">{{ error }}</p>
        <div class="actions">
          <button type="button" :disabled="busy" @click="close">Cancel</button>
          <button
            ref="chooseButton"
            type="button"
            class="primary"
            :disabled="busy"
            @click="chooseFolder"
          >
            {{ busy ? 'Adding…' : 'Choose folder…' }}<Kbd v-if="!busy" action="confirm" />
          </button>
        </div>
      </div>
      <CloneForm
        v-else-if="state.projectDialog === 'clone'"
        :workspace-id="workspaceId"
        @busy="busy = $event"
        @close="state.projectDialog = null"
      />
      <CreateRepoForm
        v-else
        :workspace-id="workspaceId"
        @busy="busy = $event"
        @close="state.projectDialog = null"
      />
    </div>
  </div>
</template>
