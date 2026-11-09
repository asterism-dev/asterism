<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { api, errorMessage } from '../api';
import { matchAction } from '../shortcuts';
import { state } from '../store';
import { defaultFolders } from '../workspaces';
import type { Workspace } from '../types';
import Kbd from './Kbd.vue';

const props = defineProps<{ mode: 'create' | 'rename' | 'delete'; workspace?: Workspace }>();
const emit = defineEmits<{ close: [] }>();

const name = ref(props.workspace?.name ?? '');
const label = ref(props.workspace?.label ?? '');
const repos = ref('');
const worktrees = ref('');
const others = computed(() => state.workspaces.filter((w) => w.id !== props.workspace?.id));
const moveTo = ref<number | undefined>(others.value[0]?.id);
const reposDefault = ref('');
const busy = ref(false);
const error = ref<string | null>(null);
const dialog = ref<HTMLElement>();

const placeholders = computed(() => defaultFolders(reposDefault.value, name.value));
const title = computed(
  () =>
    ({ create: 'New workspace', rename: 'Rename workspace', delete: 'Delete workspace' })[
      props.mode
    ],
);

function onKey(e: KeyboardEvent) {
  if (matchAction(e) !== 'confirm' || busy.value) return;
  e.preventDefault();
  dialog.value?.querySelector<HTMLButtonElement>('button[type="submit"]')?.click();
}

onMounted(() => {
  window.addEventListener('keydown', onKey);
  if (props.mode === 'create')
    api
      .nodeConfig()
      .then((info) => (reposDefault.value = info.defaults.repos))
      .catch((e) => (error.value = errorMessage(e)));
});
onUnmounted(() => window.removeEventListener('keydown', onKey));

function close() {
  if (!busy.value) emit('close');
}

async function submit() {
  busy.value = true;
  error.value = null;
  try {
    const w = props.workspace;
    if (props.mode === 'create')
      await api.createWorkspace(
        name.value.trim(),
        label.value.trim(),
        repos.value.trim() || undefined,
        worktrees.value.trim() || undefined,
      );
    else if (props.mode === 'rename' && w)
      await api.updateWorkspace({ ...w, name: name.value.trim(), label: label.value.trim() });
    else if (props.mode === 'delete' && w && moveTo.value !== undefined)
      await api.deleteWorkspace(w.id, moveTo.value);
    emit('close');
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="modal-backdrop" tabindex="-1" @click.self="close" @keydown.esc="close">
    <form
      ref="dialog"
      class="modal"
      role="dialog"
      aria-modal="true"
      :aria-label="title"
      @submit.prevent="submit"
    >
      <template v-if="mode === 'delete'">
        <p>
          Delete workspace "{{ workspace?.name }}"? Its projects move to the workspace you pick; no
          files are removed.
        </p>
        <label
          >Move projects to
          <select v-model="moveTo">
            <option v-for="w in others" :key="w.id" :value="w.id">{{ w.name }}</option>
          </select>
        </label>
      </template>
      <template v-else>
        <label
          >Name
          <input v-model="name" type="text" autofocus />
        </label>
        <label
          >Label
          <input v-model="label" type="text" maxlength="2" placeholder="e.g. W" />
        </label>
        <template v-if="mode === 'create'">
          <label
            >Repositories folder
            <input v-model="repos" type="text" :placeholder="placeholders.repos" />
          </label>
          <label
            >Worktrees folder
            <input v-model="worktrees" type="text" :placeholder="placeholders.worktrees" />
          </label>
        </template>
      </template>
      <p v-if="error" class="error">{{ error }}</p>
      <div class="actions">
        <button type="button" :disabled="busy" @click="close">Cancel</button>
        <button
          type="submit"
          :class="mode === 'delete' ? 'danger' : 'primary'"
          :disabled="
            busy ||
            (mode !== 'delete' && !name.trim()) ||
            (mode === 'delete' && moveTo === undefined)
          "
        >
          {{ mode === 'create' ? 'Create' : mode === 'rename' ? 'Rename' : 'Delete'
          }}<Kbd v-if="!busy" action="confirm" />
        </button>
      </div>
    </form>
  </div>
</template>
