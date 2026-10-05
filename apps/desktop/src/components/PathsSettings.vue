<script setup lang="ts">
import { open } from '@tauri-apps/plugin-dialog';
import { computed, onMounted, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { state, toast } from '../store';
import type { PathSettings } from '../types';

const form = ref<PathSettings>({ repos: '', worktrees: '' });
const defaults = ref<PathSettings>({ repos: '', worktrees: '' });
const saved = ref('');
const error = ref<string | null>(null);
const saving = ref(false);
const dirty = computed(() => saved.value !== '' && JSON.stringify(form.value) !== saved.value);

watch(dirty, (value) => (state.settingsDirty = value), { immediate: true });

onMounted(async () => {
  try {
    const info = await api.nodeConfig();
    form.value = { ...info.config.paths };
    defaults.value = info.defaults;
    saved.value = JSON.stringify(form.value);
  } catch (e) {
    error.value = errorMessage(e);
  }
});

async function choose(key: keyof PathSettings) {
  const picked = await open({ directory: true, multiple: false, defaultPath: form.value[key] || undefined });
  if (typeof picked === 'string') form.value[key] = picked;
}

async function save() {
  saving.value = true;
  error.value = null;
  try {
    await api.setNodeConfig({ paths: form.value });
    saved.value = JSON.stringify(form.value);
    toast('Saved paths');
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    saving.value = false;
  }
}

const fields: { key: keyof PathSettings; label: string }[] = [
  { key: 'repos', label: 'Repositories' },
  { key: 'worktrees', label: 'Worktrees' },
];
</script>

<template>
  <div class="agent-settings">
    <section v-for="f in fields" :key="f.key">
      <h3>{{ f.label }}</h3>
      <div class="settings-row">
        <input v-model="form[f.key]" spellcheck="false" />
        <button @click="choose(f.key).catch((e) => (error = errorMessage(e)))">Choose…</button>
        <button :disabled="form[f.key] === defaults[f.key]" @click="form[f.key] = defaults[f.key]">Default</button>
      </div>
    </section>
    <p class="muted">Repositories are placed in &lt;owner&gt;/&lt;repo&gt;; worktrees in &lt;owner&gt;/&lt;repo&gt;/&lt;task&gt;.</p>
    <div class="save-bar">
      <button :class="{ primary: dirty }" :disabled="saving || !dirty" @click="save">Save</button>
      <span class="muted">Applies to new repositories and tasks; existing worktrees stay where they are.</span>
    </div>
    <p v-if="error" class="error">{{ error }}</p>
  </div>
</template>
