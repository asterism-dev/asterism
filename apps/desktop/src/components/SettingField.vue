<script setup lang="ts">
import { Button, Field, NumberInput, Select, Switch, TextInput } from '@asterism-dev/lumen';
import type { SettingsDraft } from '../pluginsView';
import type { SettingSpec, SettingValue } from '../types';

const props = defineProps<{ spec: SettingSpec; secretSet?: boolean }>();
const draft = defineModel<SettingsDraft>('draft', { required: true });

const enumOptions = () => (props.spec.options ?? []).map((o) => ({ value: o, label: o }));
const numberValue = (v: SettingValue) => (typeof v === 'number' ? v : null);

// An empty number is kept as '' like other empty fields, which the patch turns into "use default"
function setValue(value: SettingValue | null) {
  draft.value.values[props.spec.key] = value ?? '';
}
</script>

<template>
  <Field
    :label="spec.required ? `${spec.title} *` : spec.title"
    :hint="spec.description ?? undefined"
  >
    <div v-if="spec.type === 'secret'" class="settings-row">
      <TextInput
        v-model="draft.secrets[spec.key]"
        type="password"
        autocomplete="off"
        :placeholder="
          secretSet && !draft.cleared.includes(spec.key) ? 'set — type to replace' : 'not set'
        "
      />
      <Button v-if="secretSet" @click="draft.cleared.push(spec.key)"> Clear </Button>
    </div>
    <Switch
      v-else-if="spec.type === 'bool'"
      :model-value="draft.values[spec.key] === true"
      @update:model-value="setValue($event)"
    />
    <NumberInput
      v-else-if="spec.type === 'number'"
      :model-value="numberValue(draft.values[spec.key])"
      @update:model-value="setValue($event)"
    />
    <Select
      v-else-if="spec.type === 'enum'"
      :model-value="String(draft.values[spec.key])"
      :options="enumOptions()"
      @update:model-value="setValue($event)"
    />
    <TextInput
      v-else
      :model-value="String(draft.values[spec.key])"
      spellcheck="false"
      @update:model-value="setValue($event)"
    />
  </Field>
</template>
