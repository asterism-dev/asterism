<script setup lang="ts">
import { Button, Field, NumberInput, Select, Switch, TextInput } from '@asterism-dev/lumen';
import { computed, ref, watch } from 'vue';
import { api, errorMessage } from '../../api';
import { draftFrom, settingsPatch, type SettingsDraft } from '../../pluginsView';
import type { PluginSettings, SettingSpec, SettingValue } from '../../types';

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

const enumOptions = (spec: SettingSpec) =>
  (spec.options ?? []).map((o) => ({ value: o, label: o }));

const numberValue = (v: SettingValue) => (typeof v === 'number' ? v : null);

// An empty number is kept as '' like other empty fields, which the patch turns into "use default"
function setValue(key: string, value: SettingValue | null) {
  if (draft.value) draft.value.values[key] = value ?? '';
}

defineExpose({ dirty, save });

watch(() => props.plugin, load, { immediate: true });
</script>

<template>
  <p v-if="error" class="error">{{ error }}</p>
  <form v-if="settings && draft && settings.schema.length" class="add-form" @submit.prevent>
    <Field
      v-for="spec in settings.schema"
      :key="spec.key"
      :label="spec.required ? `${spec.title} *` : spec.title"
      :hint="spec.description ?? undefined"
    >
      <div v-if="spec.type === 'secret'" class="settings-row">
        <TextInput
          v-model="draft.secrets[spec.key]"
          type="password"
          autocomplete="off"
          :placeholder="
            settings.secrets_set.includes(spec.key) && !draft.cleared.includes(spec.key)
              ? 'set — type to replace'
              : 'not set'
          "
        />
        <Button
          v-if="settings.secrets_set.includes(spec.key)"
          @click="draft.cleared.push(spec.key)"
        >
          Clear
        </Button>
      </div>
      <Switch
        v-else-if="spec.type === 'bool'"
        :model-value="draft.values[spec.key] === true"
        @update:model-value="setValue(spec.key, $event)"
      />
      <NumberInput
        v-else-if="spec.type === 'number'"
        :model-value="numberValue(draft.values[spec.key])"
        @update:model-value="setValue(spec.key, $event)"
      />
      <Select
        v-else-if="spec.type === 'enum'"
        :model-value="String(draft.values[spec.key])"
        :options="enumOptions(spec)"
        @update:model-value="setValue(spec.key, $event)"
      />
      <TextInput
        v-else
        :model-value="String(draft.values[spec.key])"
        spellcheck="false"
        @update:model-value="setValue(spec.key, $event)"
      />
    </Field>
  </form>
</template>
