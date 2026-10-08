<script setup lang="ts">
import { open } from '@tauri-apps/plugin-dialog';
import { onMounted, onUnmounted, ref } from 'vue';
import { api, errorMessage } from '../api';
import { matchAction } from '../shortcuts';
import { state, type ProjectDialogTab } from '../store';
import CloneForm from './CloneForm.vue';
import CreateRepoForm from './CreateRepoForm.vue';
import Kbd from './Kbd.vue';

const TABS: { id: ProjectDialogTab; label: string }[] = [
  { id: 'folder', label: 'Add folder' },
  { id: 'clone', label: 'Clone repository' },
  { id: 'create', label: 'New repository' },
];

const busy = ref(false);
const error = ref<string | null>(null);
const chooseButton = ref<HTMLButtonElement>();
const dialog = ref<HTMLElement>();

// On window, like NewTaskDialog: WebKit doesn't focus clicked buttons, so focus often sits on body.
function onKey(e: KeyboardEvent) {
  if (matchAction(e) !== 'confirm' || busy.value) return;
  e.preventDefault();
  if (state.projectDialog === 'folder') void chooseFolder();
  // Clicking (not requestSubmit) respects the button's disabled state and works on Safari 15.
  else dialog.value?.querySelector<HTMLButtonElement>('button[type="submit"]')?.click();
}

onMounted(() => {
  chooseButton.value?.focus();
  window.addEventListener('keydown', onKey);
});
onUnmounted(() => window.removeEventListener('keydown', onKey));

function close() {
  if (!busy.value) state.projectDialog = null;
}

function select(tab: ProjectDialogTab) {
  if (!busy.value) state.projectDialog = tab;
}

async function chooseFolder() {
  error.value = null;
  const path = await open({ directory: true, multiple: false });
  if (typeof path !== 'string') return;
  busy.value = true;
  try {
    const project = await api.addProject(path);
    delete state.collapsed[project.id];
    state.projectDialog = null;
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
      <div v-if="state.projectDialog === 'folder'" class="add-form">
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
        @busy="busy = $event"
        @close="state.projectDialog = null"
      />
      <CreateRepoForm v-else @busy="busy = $event" @close="state.projectDialog = null" />
    </div>
  </div>
</template>
