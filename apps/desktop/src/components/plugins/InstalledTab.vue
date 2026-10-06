<script setup lang="ts">
import { ask, open as openDialog } from '@tauri-apps/plugin-dialog';
import { computed, onMounted, ref, watch } from 'vue';
import { api, errorMessage, RpcError } from '../../api';
import {
  capabilityChips, draftFrom, newPermissions, originLabel, permissionText, settingsPatch, stateDetail, statusLabel, updateCount,
  type SettingsDraft,
} from '../../pluginsView';
import { state, toast } from '../../store';
import type { PluginInfo, PluginSettings } from '../../types';

const props = defineProps<{ configureRequest: string | null }>();
const emit = defineEmits<{ configured: [] }>();

const plugins = ref<PluginInfo[]>([]);
const error = ref<string | null>(null);
const open = ref<string | null>(null);
const settings = ref<PluginSettings | null>(null);
const draft = ref<SettingsDraft | null>(null);
const saving = ref(false);
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

async function configure(name: string) {
  if (open.value === name) {
    open.value = null;
    return;
  }
  try {
    settings.value = await api.pluginSettings(name);
    draft.value = draftFrom(settings.value);
    open.value = name;
  } catch (e) {
    toast(errorMessage(e));
  }
}

async function save() {
  if (!open.value || !settings.value || !draft.value) return;
  saving.value = true;
  try {
    await api.setPluginSettings(open.value, settingsPatch(settings.value, draft.value));
    toast(`Saved ${open.value} settings`);
    open.value = null;
  } catch (e) {
    toast(errorMessage(e));
  } finally {
    saving.value = false;
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
  if (await ask(`Uninstall ${p.name}? Its settings are kept.`, { title: 'Uninstall plugin', kind: 'warning' })) {
    await act(p.name, () => api.uninstallPlugin(p.name));
  }
}

async function linkLocal() {
  const path = await openDialog({ directory: true, multiple: false });
  if (typeof path !== 'string') return;
  try {
    const linked = await api.linkPlugin(path);
    toast(`Linked ${linked.name}`);
  } catch (e) {
    toast(errorMessage(e));
  }
}

onMounted(load);
watch(() => state.pluginsVersion, load);
watch(
  () => props.configureRequest,
  async (name) => {
    if (!name) return;
    await load();
    await configure(name);
    emit('configured');
  },
  { immediate: true },
);
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
            <button v-if="p.state.state !== 'broken' && p.state.state !== 'disabled'" @click="configure(p.name)">Configure</button>
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
        <form v-if="open === p.name && settings && draft" class="add-form" @submit.prevent="save">
          <p v-if="!settings.schema.length" class="muted">This plugin has no settings.</p>
          <label v-for="spec in settings.schema" :key="spec.key">
            {{ spec.title }}<span v-if="spec.required"> *</span>
            <template v-if="spec.type === 'secret'">
              <input
                v-model="draft.secrets[spec.key]"
                type="password"
                autocomplete="off"
                :placeholder="settings.secrets_set.includes(spec.key) && !draft.cleared.includes(spec.key) ? 'set — type to replace' : 'not set'"
              />
              <button v-if="settings.secrets_set.includes(spec.key)" type="button" @click="draft.cleared.push(spec.key)">Clear</button>
            </template>
            <input v-else-if="spec.type === 'bool'" v-model="draft.values[spec.key]" type="checkbox" />
            <input v-else-if="spec.type === 'number'" v-model.number="draft.values[spec.key]" type="number" />
            <select v-else-if="spec.type === 'enum'" v-model="draft.values[spec.key]">
              <option v-for="o in spec.options ?? []" :key="o" :value="o">{{ o }}</option>
            </select>
            <input v-else v-model="draft.values[spec.key]" spellcheck="false" />
            <span v-if="spec.description" class="muted">{{ spec.description }}</span>
          </label>
          <div class="actions">
            <button type="button" @click="open = null">Cancel</button>
            <button type="submit" :disabled="saving">{{ saving ? 'Saving…' : 'Save' }}</button>
          </div>
        </form>
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
