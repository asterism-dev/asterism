<script setup lang="ts">
import { onMounted, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { capabilityChips, draftFrom, settingsPatch, stateDetail, stateLabel, type SettingsDraft } from '../pluginsView';
import { state, toast } from '../store';
import type { PluginInfo, PluginSettings } from '../types';

const plugins = ref<PluginInfo[]>([]);
const error = ref<string | null>(null);
const open = ref<string | null>(null);
const settings = ref<PluginSettings | null>(null);
const draft = ref<SettingsDraft | null>(null);
const saving = ref(false);

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

async function reload() {
  try {
    await api.reloadPlugins();
  } catch (e) {
    toast(errorMessage(e));
  }
}

onMounted(load);
watch(() => state.pluginsVersion, load);
</script>

<template>
  <section>
    <div class="plugins-header">
      <h3>Plugins</h3>
      <button @click="reload">Reload</button>
    </div>
    <p v-if="error" class="error">{{ error }}</p>
    <ul class="plugin-list">
      <li v-for="p in plugins" :key="p.name">
        <div class="plugin-row">
          <strong>{{ p.name }}</strong>
          <span class="muted">{{ p.version ?? '?' }} · {{ p.origin === 'linked' ? 'linked (dev)' : 'built-in' }}</span>
          <span v-if="stateLabel(p.state)" class="plugin-state" :title="stateDetail(p.state) ?? ''">{{ stateLabel(p.state) }}</span>
          <span class="spacer" />
          <button v-if="p.state.state !== 'broken'" @click="configure(p.name)">Configure</button>
        </div>
        <p v-if="p.description" class="muted">{{ p.description }}</p>
        <p v-if="stateDetail(p.state)" class="muted">{{ stateDetail(p.state) }}</p>
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
</style>
