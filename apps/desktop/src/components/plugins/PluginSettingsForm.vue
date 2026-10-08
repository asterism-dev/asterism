<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { api, errorMessage } from '../../api';
import { draftFrom, settingsPatch, type SettingsDraft } from '../../pluginsView';
import type { PluginSettings } from '../../types';

const props = defineProps<{ plugin: string }>();

const settings = ref<PluginSettings | null>(null);
const draft = ref<SettingsDraft | null>(null);
const error = ref<string | null>(null);
const saved = ref('null');
const dirty = computed(() => JSON.stringify(draft.value) !== saved.value);

async function load() {
  error.value = null;
  try {
    settings.value = await api.pluginSettings(props.plugin);
    draft.value = draftFrom(settings.value);
    saved.value = JSON.stringify(draft.value);
  } catch (e) {
    error.value = errorMessage(e);
  }
}

async function save(): Promise<boolean> {
  if (!settings.value || !draft.value) return false;
  try {
    await api.setPluginSettings(props.plugin, settingsPatch(settings.value, draft.value));
    await load();
    return true;
  } catch (e) {
    error.value = errorMessage(e);
    return false;
  }
}

defineExpose({ dirty, save });

watch(() => props.plugin, load, { immediate: true });
</script>

<template>
  <p v-if="error" class="error">{{ error }}</p>
  <form v-if="settings && draft && settings.schema.length" class="add-form" @submit.prevent>
    <label v-for="spec in settings.schema" :key="spec.key">
      {{ spec.title }}<span v-if="spec.required"> *</span>
      <template v-if="spec.type === 'secret'">
        <input
          v-model="draft.secrets[spec.key]"
          type="password"
          autocomplete="off"
          :placeholder="
            settings.secrets_set.includes(spec.key) && !draft.cleared.includes(spec.key)
              ? 'set — type to replace'
              : 'not set'
          "
        />
        <button
          v-if="settings.secrets_set.includes(spec.key)"
          type="button"
          @click="draft.cleared.push(spec.key)"
        >
          Clear
        </button>
      </template>
      <input v-else-if="spec.type === 'bool'" v-model="draft.values[spec.key]" type="checkbox" />
      <input
        v-else-if="spec.type === 'number'"
        v-model.number="draft.values[spec.key]"
        type="number"
      />
      <select v-else-if="spec.type === 'enum'" v-model="draft.values[spec.key]">
        <option v-for="o in spec.options ?? []" :key="o" :value="o">{{ o }}</option>
      </select>
      <input v-else v-model="draft.values[spec.key]" spellcheck="false" />
      <span v-if="spec.description" class="muted">{{ spec.description }}</span>
    </label>
  </form>
</template>
