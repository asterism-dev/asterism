<script setup lang="ts">
import { ask, open as openDialog } from '@tauri-apps/plugin-dialog';
import { computed, onMounted, ref, watch } from 'vue';
import { api, errorMessage, RpcError } from '../../api';
import { capabilityChips, newPermissions, originLabel, permissionText, stateDetail, statusLabel, updateCount } from '../../pluginsView';
import { state, toast } from '../../store';
import type { PluginInfo } from '../../types';

const plugins = ref<PluginInfo[]>([]);
const error = ref<string | null>(null);
const busy = ref<Record<string, boolean>>({});
const rowError = ref<Record<string, string>>({});
const updates = computed(() => updateCount(plugins.value));

async function load() {
  error.value = null;
  try {
    plugins.value = await api.plugins();
  } catch (e) {
    error.value = errorMessage(e);
  }
}

/** Runs a row action with per-row busy state; errors stay at the row. */
async function act(name: string, action: () => Promise<unknown>) {
  busy.value[name] = true;
  delete rowError.value[name];
  try {
    await action();
  } catch (e) {
    rowError.value[name] = errorMessage(e);
    await load();
  } finally {
    busy.value[name] = false;
  }
}

async function update(p: PluginInfo) {
  try {
    await api.updatePlugin(p.name);
  } catch (e) {
    if (!(e instanceof RpcError && e.kind === 'permissions_changed') || !p.store) throw e;
    const offered = await api.pluginDetails(p.store, p.name);
    const added = newPermissions(p.permissions, offered.permissions).map(permissionText);
    const question = `${p.name} ${offered.version} asks for new permissions:\n\n${added.join('\n')}\n\nUpdate?`;
    if (await ask(question, { title: 'Update plugin', kind: 'warning' })) await api.updatePlugin(p.name, offered.permissions);
  }
}

const rollback = (name: string) => api.rollbackPlugin(name);
const setEnabled = (p: PluginInfo) => api.setPluginEnabled(p.name, p.state.state === 'disabled');
const unlink = (name: string) => api.unlinkPlugin(name);

async function updateAll() {
  for (const p of plugins.value.filter((x) => x.update_available)) await act(p.name, () => update(p));
}

async function uninstall(p: PluginInfo) {
  await act(p.name, async () => {
    if (await ask(`Uninstall ${p.name}? Its settings are kept.`, { title: 'Uninstall plugin', kind: 'warning' })) {
      await api.uninstallPlugin(p.name);
    }
  });
}

async function linkLocal() {
  try {
    const path = await openDialog({ directory: true, multiple: false });
    if (typeof path !== 'string') return;
    const linked = await api.linkPlugin(path);
    toast(`Linked ${linked.name}`);
  } catch (e) {
    toast(errorMessage(e));
  }
}

onMounted(load);
watch(() => [state.pluginsVersion, state.storesVersion], load);
</script>

<template>
  <section>
    <div class="plugins-header">
      <h3>Installed</h3>
      <span class="spacer" />
      <button v-if="updates" @click="updateAll">Update all ({{ updates }})</button>
      <button @click="linkLocal">Link local plugin…</button>
      <button @click="api.reloadPlugins().catch((e) => toast(errorMessage(e)))">Reload</button>
    </div>
    <p v-if="error" class="error">{{ error }}</p>
    <ul class="plugin-list">
      <li v-for="p in plugins" :key="p.name">
        <div class="plugin-row">
          <strong>{{ p.name }}</strong>
          <span class="muted">{{ p.version ?? '?' }} · {{ originLabel(p) }}</span>
          <span v-if="statusLabel(p)" class="plugin-state" :title="stateDetail(p.state) ?? ''">{{ statusLabel(p) }}</span>
          <span class="spacer" />
          <template v-if="!busy[p.name]">
            <button v-if="p.update_available" @click="act(p.name, () => update(p))">Update</button>
            <button v-if="p.previous_version" :title="`Back to ${p.previous_version}`" @click="act(p.name, () => rollback(p.name))">Rollback</button>
            <label class="toggle">
              <input type="checkbox" :checked="p.state.state !== 'disabled'" @change="act(p.name, () => setEnabled(p))" />
              Enabled
            </label>
            <button v-if="p.state.state !== 'broken' && p.state.state !== 'disabled'" @click="state.pluginSettingsRequest = p.name">Configure</button>
            <button v-if="p.origin === 'installed'" @click="uninstall(p)">Uninstall</button>
            <button v-if="p.origin === 'linked'" @click="act(p.name, () => unlink(p.name))">Unlink</button>
          </template>
          <span v-else class="muted">Working…</span>
        </div>
        <p v-if="p.description" class="muted">{{ p.description }}</p>
        <p v-if="stateDetail(p.state)" class="muted">{{ stateDetail(p.state) }}</p>
        <p v-if="rowError[p.name]" class="error">{{ rowError[p.name] }}</p>
        <div class="chips">
          <span v-for="c in capabilityChips(p)" :key="c" class="chip">{{ c }}</span>
        </div>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.plugins-header { display: flex; align-items: center; justify-content: space-between; }
.plugin-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 12px; }
.plugin-row { display: flex; align-items: center; gap: 8px; }
.plugin-row .spacer { flex: 1; }
.plugin-state { font-size: 11px; padding: 1px 6px; border-radius: 8px; background: var(--select); }
.chips { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 4px; }
.chip { font-size: 11px; padding: 1px 6px; border: 1px solid var(--border); border-radius: 8px; }
.plugins-header .spacer { flex: 1; }
.toggle { display: flex; align-items: center; gap: 4px; }
</style>
