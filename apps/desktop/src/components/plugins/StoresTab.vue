<script setup lang="ts">
import { ask, open as openDialog } from '@tauri-apps/plugin-dialog';
import { onMounted, ref, watch } from 'vue';
import { api, errorMessage } from '../../api';
import { relativeTime } from '../../projects';
import { state, toast } from '../../store';
import type { StoreList } from '../../types';

const TRUST_WARNING =
  'Plugins from this store run code on your machine. Only add stores you trust.';
const list = ref<StoreList | null>(null);
const error = ref<string | null>(null);
const source = ref('');
const adding = ref(false);
const busy = ref<Record<string, boolean>>({});
const rowError = ref<Record<string, string>>({});

async function load() {
  error.value = null;
  try {
    list.value = await api.storeList();
  } catch (e) {
    error.value = errorMessage(e);
  }
}

const OFFICIAL_SOURCE = 'https://github.com/asterism-dev/asterism-plugins.git';

async function add() {
  const value = source.value.trim();
  if (!value) return;
  try {
    if (
      value !== OFFICIAL_SOURCE &&
      !(await ask(`${TRUST_WARNING}\n\nAdd ${value}?`, { title: 'Add store', kind: 'warning' }))
    )
      return;
  } catch (e) {
    toast(errorMessage(e));
    return;
  }
  adding.value = true;
  try {
    const store = await api.addStore(value);
    toast(`Added store ${store.name}`);
    source.value = '';
  } catch (e) {
    toast(errorMessage(e));
  } finally {
    adding.value = false;
  }
}

async function pickFolder() {
  try {
    const path = await openDialog({ directory: true, multiple: false });
    if (typeof path === 'string') source.value = path;
  } catch (e) {
    toast(errorMessage(e));
  }
}

async function act(name: string, action: () => Promise<unknown>) {
  busy.value[name] = true;
  delete rowError.value[name];
  try {
    await action();
  } catch (e) {
    rowError.value[name] = errorMessage(e);
  } finally {
    busy.value[name] = false;
  }
}

async function remove(name: string) {
  try {
    if (
      !(await ask(`Remove store ${name}? Plugins installed from it are uninstalled too.`, {
        title: 'Remove store',
        kind: 'warning',
      }))
    )
      return;
  } catch (e) {
    rowError.value[name] = errorMessage(e);
    return;
  }
  await act(name, () => api.removeStore(name, true));
}

function refresh(name: string) {
  return act(name, () => api.refreshStores(name));
}

function toggleAutoUpdate() {
  api.setAutoUpdate(!(list.value?.auto_update ?? false)).catch((e) => toast(errorMessage(e)));
}

onMounted(load);
watch(() => [state.storesVersion, state.pluginsVersion], load);
</script>

<template>
  <section>
    <h3>Stores</h3>
    <p v-if="error" class="error">{{ error }}</p>
    <p v-if="list?.error" class="error">{{ list.error }}</p>
    <ul class="store-list">
      <li v-for="s in list?.stores ?? []" :key="s.name">
        <div class="store-row">
          <strong>{{ s.name }}</strong>
          <span v-if="s.official" class="plugin-state">official</span>
          <span class="muted"
            >{{ s.plugin_count }} plugins ·
            {{
              s.last_refreshed
                ? `refreshed ${relativeTime(s.last_refreshed, Math.floor(Date.now() / 1000))} ago`
                : 'not refreshed yet'
            }}</span
          >
          <span class="spacer" />
          <template v-if="!busy[s.name]">
            <button @click="refresh(s.name)">Refresh</button>
            <button @click="remove(s.name)">Remove</button>
          </template>
          <span v-else class="muted">Working…</span>
        </div>
        <p class="muted mono">{{ s.source }}</p>
        <p v-if="s.last_error" class="error">{{ s.last_error }}</p>
        <p v-if="rowError[s.name]" class="error">{{ rowError[s.name] }}</p>
      </li>
    </ul>
    <form class="add-form" @submit.prevent="add">
      <label
        >Add store
        <input
          v-model="source"
          placeholder="https://…/plugins.git or a local folder"
          spellcheck="false"
          :disabled="adding"
        />
      </label>
      <div class="actions">
        <button type="button" :disabled="adding" @click="pickFolder">Choose folder…</button>
        <button type="submit" :disabled="adding || !source.trim()">
          {{ adding ? 'Adding…' : 'Add store' }}
        </button>
      </div>
    </form>
    <label class="toggle">
      <input type="checkbox" :checked="list?.auto_update ?? false" @change="toggleAutoUpdate" />
      Install updates automatically
    </label>
    <p class="muted">Updates that ask for new permissions always wait for you.</p>
  </section>
</template>

<style scoped>
.store-list {
  list-style: none;
  margin: 0 0 12px;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.store-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.store-row .spacer {
  flex: 1;
}
.plugin-state {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 8px;
  background: var(--select);
}
.toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 12px;
}
</style>
