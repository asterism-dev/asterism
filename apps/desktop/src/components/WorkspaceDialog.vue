<script setup lang="ts">
import { Button, Dialog, Field, Select, TextInput } from '@asterism-dev/lumen';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { api, errorMessage } from '../api';
import { matchAction } from '../shortcuts';
import { state, switchWorkspace } from '../store';
import { WORKSPACE_COLORS, colorVar, defaultFolders, nextFreeColor } from '../workspaces';
import type { Workspace } from '../types';
import Kbd from './Kbd.vue';

const props = defineProps<{ mode: 'create' | 'rename' | 'delete'; workspace?: Workspace }>();
const emit = defineEmits<{ close: [] }>();

const name = ref(props.workspace?.name ?? '');
const label = ref(props.workspace?.label ?? '');
const repos = ref('');
const worktrees = ref('');
const color = ref(props.workspace?.color ?? nextFreeColor(state.workspaces));
const others = computed(() => state.workspaces.filter((w) => w.id !== props.workspace?.id));
const moveTo = ref<number | undefined>(others.value[0]?.id);
const moveOptions = computed(() => others.value.map((w) => ({ value: w.id, label: w.name })));
const reposDefault = ref('');
const busy = ref(false);
const error = ref<string | null>(null);
const dialog = ref<HTMLElement>();

const placeholders = computed(() => defaultFolders(reposDefault.value, name.value));
const title = computed(
  () =>
    ({ create: 'New workspace', rename: 'Edit workspace', delete: 'Delete workspace' })[props.mode],
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
        color.value,
        repos.value.trim() || undefined,
        worktrees.value.trim() || undefined,
      );
    else if (props.mode === 'rename' && w)
      await api.updateWorkspace({
        ...w,
        name: name.value.trim(),
        label: label.value.trim(),
        color: color.value,
      });
    else if (props.mode === 'delete' && w && moveTo.value !== undefined) {
      const wasActive = state.activeWorkspace === w.id;
      const selected = state.selectedTaskId;
      await api.deleteWorkspace(w.id, moveTo.value);
      if (wasActive) {
        switchWorkspace(state, moveTo.value);
        if (selected !== null && state.tasks.some((t) => t.id === selected))
          state.selectedTaskId = selected;
      }
    }
    emit('close');
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <Dialog :open="true" :title="title" @update:open="(o) => !o && close()">
    <form ref="dialog" class="workspace-form" @submit.prevent="submit">
      <template v-if="mode === 'delete'">
        <p>
          Delete workspace "{{ workspace?.name }}"? Its projects move to the workspace you pick; no
          files are removed.
        </p>
        <Field label="Move projects to">
          <Select
            :model-value="moveTo as number"
            :options="moveOptions"
            @update:model-value="moveTo = $event"
          />
        </Field>
      </template>
      <template v-else>
        <Field label="Name">
          <TextInput v-model="name" />
        </Field>
        <Field label="Label">
          <TextInput v-model="label" maxlength="2" placeholder="e.g. W" />
        </Field>
        <fieldset class="colors">
          <legend>Color</legend>
          <label
            v-for="c in ['', ...WORKSPACE_COLORS]"
            :key="c"
            class="swatch"
            :title="c || 'none'"
          >
            <input v-model="color" type="radio" name="color" :value="c" :aria-label="c || 'none'" />
            <span :style="{ background: colorVar(c) }" :class="{ none: !c }" />
          </label>
        </fieldset>
        <template v-if="mode === 'create'">
          <Field label="Repositories folder">
            <TextInput v-model="repos" :placeholder="placeholders.repos" />
          </Field>
          <Field label="Worktrees folder">
            <TextInput v-model="worktrees" :placeholder="placeholders.worktrees" />
          </Field>
        </template>
      </template>
      <p v-if="error" class="error">{{ error }}</p>
      <div class="actions">
        <Button :disabled="busy" @click="close">Cancel</Button>
        <Button
          type="submit"
          :variant="mode === 'delete' ? 'danger' : 'accent'"
          :disabled="
            busy ||
            (mode !== 'delete' && !name.trim()) ||
            (mode === 'delete' && moveTo === undefined)
          "
        >
          {{ mode === 'create' ? 'Create' : mode === 'rename' ? 'Save' : 'Delete'
          }}<Kbd v-if="!busy" action="confirm" />
        </Button>
      </div>
    </form>
  </Dialog>
</template>

<style scoped>
.workspace-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.colors {
  display: flex;
  gap: 6px;
  border: 0;
  padding: 0;
  margin: 0;
}
.colors legend {
  padding: 0 0 4px;
}
.swatch input {
  position: absolute;
  opacity: 0;
}
.swatch span {
  display: block;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 2px solid transparent;
  cursor: pointer;
}
.swatch span.none {
  border-color: var(--border);
  background: linear-gradient(135deg, transparent 45%, var(--muted) 45% 55%, transparent 55%);
}
.swatch input:checked + span {
  outline: 2px solid var(--text);
  outline-offset: 2px;
}
.swatch input:focus-visible + span {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
</style>
