<script setup lang="ts">
import { ref, watch } from 'vue';
import { api, errorMessage } from '../../api';
import { draftFrom, settingsPatch, type SettingsDraft } from '../../pluginsView';
import { toast } from '../../store';
import type { PluginSettings } from '../../types';

const props = defineProps<{ plugin: string }>();

const settings = ref<PluginSettings | null>(null);
const draft = ref<SettingsDraft | null>(null);
const error = ref<string | null>(null);
const saving = ref(false);

async function load() {
  error.value = null;
  try {
    settings.value = await api.pluginSettings(props.plugin);
    draft.value = draftFrom(settings.value);
  } catch (e) {
    error.value = errorMessage(e);
  }
}

async function save() {
  if (!settings.value || !draft.value) return;
  saving.value = true;
  try {
    await api.setPluginSettings(props.plugin, settingsPatch(settings.value, draft.value));
    toast(`Saved ${props.plugin} settings`);
    await load();
  } catch (e) {
    toast(errorMessage(e));
  } finally {
    saving.value = false;
  }
}

watch(() => props.plugin, load, { immediate: true });
</script>

<template>
  <p v-if="error" class="error">{{ error }}</p>
  <form v-if="settings && draft && settings.schema.length" class="add-form" @submit.prevent="save">
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
      <button type="submit" :disabled="saving">{{ saving ? 'Saving…' : 'Save' }}</button>
    </div>
  </form>
</template>
