<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { api, errorMessage } from '../../api';
import { draftFrom, settingsPatch, type SettingsDraft } from '../../pluginsView';
import type { PluginSettings } from '../../types';
import SettingField from '../SettingField.vue';

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
    <SettingField
      v-for="spec in settings.schema"
      :key="spec.key"
      :spec="spec"
      :draft="draft"
      :secret-set="settings.secrets_set.includes(spec.key)"
    />
  </form>
</template>
