<script setup lang="ts">
import { open } from '@tauri-apps/plugin-dialog';
import { computed, onMounted, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { state, toast } from '../store';
import type { PathSettings } from '../types';

const drafts = ref<Record<number, PathSettings>>({});
const baseline = ref<Record<number, PathSettings>>({});
const globals = ref<PathSettings>({ repos: '', worktrees: '' });
const loaded = ref(false);
const error = ref<string | null>(null);
const saving = ref(false);

const changed = computed(() =>
  state.workspaces.filter((w) => {
    const draft = drafts.value[w.id];
    const base = baseline.value[w.id];
    return draft && base && (draft.repos !== base.repos || draft.worktrees !== base.worktrees);
  }),
);
const dirty = computed(() => changed.value.length > 0);

watch(dirty, (value) => (state.settingsDirty = value), { immediate: true });
watch(() => state.workspaces, syncNew, { immediate: true, deep: true });

onMounted(load);

function syncNew() {
  for (const w of state.workspaces) {
    if (drafts.value[w.id]) continue;
    drafts.value[w.id] = { repos: w.repos, worktrees: w.worktrees };
    baseline.value[w.id] = { repos: w.repos, worktrees: w.worktrees };
  }
}

async function load() {
  error.value = null;
  try {
    globals.value = (await api.nodeConfig()).config.paths;
    loaded.value = true;
  } catch (e) {
    error.value = errorMessage(e);
  }
}

async function choose(id: number, key: keyof PathSettings) {
  error.value = null;
  const picked = await open({
    directory: true,
    multiple: false,
    defaultPath: drafts.value[id][key] || globals.value[key] || undefined,
  });
  if (typeof picked === 'string') drafts.value[id][key] = picked;
}

async function save() {
  saving.value = true;
  error.value = null;
  for (const w of changed.value) {
    try {
      const { repos, worktrees } = drafts.value[w.id];
      await api.updateWorkspace({ ...w, repos, worktrees });
      baseline.value[w.id] = { repos, worktrees };
    } catch (e) {
      error.value = `${w.name}: ${errorMessage(e)}`;
      break;
    }
  }
  saving.value = false;
  if (!error.value) toast('Saved paths');
}

const fields: { key: keyof PathSettings; label: string }[] = [
  { key: 'repos', label: 'Repositories' },
  { key: 'worktrees', label: 'Worktrees' },
];
</script>

<template>
  <div class="agent-settings">
    <section v-for="w in state.workspaces" :key="w.id">
      <h3>{{ w.name }}</h3>
      <template v-if="drafts[w.id]">
        <div v-for="f in fields" :key="f.key" class="settings-row">
          <label :for="`path-${w.id}-${f.key}`">{{ f.label }}</label>
          <input
            :id="`path-${w.id}-${f.key}`"
            v-model="drafts[w.id][f.key]"
            :placeholder="globals[f.key]"
            spellcheck="false"
          />
          <button @click="choose(w.id, f.key).catch((e) => (error = errorMessage(e)))">
            Choose…
          </button>
          <span v-if="!drafts[w.id][f.key]" class="muted">global default</span>
        </div>
      </template>
    </section>
    <p class="muted">
      Repositories are placed in &lt;owner&gt;/&lt;repo&gt;; worktrees in
      &lt;owner&gt;/&lt;repo&gt;/&lt;task&gt;.
    </p>
    <div class="save-bar">
      <button :class="{ primary: dirty }" :disabled="saving || !dirty" @click="save">Save</button>
      <span class="muted"
        >Applies to new repositories and tasks; existing worktrees stay where they are.</span
      >
    </div>
    <p v-if="error" class="error">{{ error }}</p>
    <button v-if="error && !loaded" @click="load">Retry</button>
  </div>
</template>
