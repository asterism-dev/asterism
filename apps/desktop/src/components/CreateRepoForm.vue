<script setup lang="ts">
import { Button, Field, Select, TextInput } from '@asterism-dev/lumen';
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { repoNameError, targetPath, visibilityChoices } from '../projects';
import { state, toast } from '../store';
import Kbd from './Kbd.vue';
import type { ForgeInfo, ForgeStatus, Visibility } from '../types';

const props = defineProps<{ workspaceId: number | null }>();
const emit = defineEmits<{ close: []; busy: [boolean] }>();

const name = ref('');
const onRemote = ref(false);
const forges = ref<ForgeInfo[]>([]);
const forge = ref('');
const status = ref<ForgeStatus | null>(null);
const owner = ref('');
const visibility = ref<Visibility>('private');
const defaultRepos = ref('');
const reposRoot = computed(
  () => state.workspaces.find((w) => w.id === props.workspaceId)?.repos || defaultRepos.value,
);
const error = ref<string | null>(null);
const busy = ref(false);
const input = ref<InstanceType<typeof TextInput>>();

const owners = computed(() =>
  status.value?.account ? [status.value.account, ...status.value.owners] : [],
);
const forgeOptions = computed(() =>
  forges.value.map((f) => ({ value: f.id, label: f.display_name })),
);
const ownerOptions = computed(() => owners.value.map((o) => ({ value: o, label: o })));
const forgeName = computed(
  () => forges.value.find((f) => f.id === forge.value)?.display_name ?? 'remote',
);
const choices = computed(() => (status.value ? visibilityChoices(owner.value, status.value) : []));
const visibilityOptions = computed(() => choices.value.map((v) => ({ value: v, label: v })));
const nameError = computed(() => (name.value ? repoNameError(name.value) : null));
const preview = computed(() =>
  name.value && !nameError.value && reposRoot.value
    ? targetPath(reposRoot.value, onRemote.value ? owner.value : 'local', name.value)
    : '',
);

watch(choices, (list) => {
  if (!list.includes(visibility.value)) visibility.value = 'private';
});

onMounted(async () => {
  input.value?.$el.focus();
  try {
    const [list, config] = await Promise.all([api.forges(), api.nodeConfig()]);
    forges.value = list;
    defaultRepos.value = config.config.paths.repos;
    forge.value = list[0]?.id ?? '';
  } catch (e) {
    error.value = errorMessage(e);
  }
});

watch(forge, async (f) => {
  status.value = null;
  owner.value = '';
  onRemote.value = false;
  if (!f) return;
  try {
    const next = await api.forgeStatus(f);
    if (forge.value !== f) return;
    status.value = next;
    owner.value = next.account ?? '';
  } catch (e) {
    if (forge.value === f) error.value = errorMessage(e);
  }
});

async function create() {
  const invalid = repoNameError(name.value);
  if (invalid) {
    error.value = invalid;
    return;
  }
  busy.value = true;
  error.value = null;
  try {
    const remote = onRemote.value
      ? { forge: forge.value, owner: owner.value, visibility: visibility.value }
      : null;
    const result = await api.createProject(name.value, remote, props.workspaceId ?? undefined);
    delete state.collapsed[result.project.id];
    if (result.remote_error)
      toast(`Created locally; ${forgeName.value} failed: ${result.remote_error}`);
    else toast(`Created ${name.value}`);
    emit('close');
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
    if (error.value) {
      await nextTick();
      input.value?.$el.focus();
    }
  }
}
watch(busy, (b) => emit('busy', b));
</script>

<template>
  <form class="add-form" @submit.prevent="create">
    <Field label="Name" :error="nameError ?? undefined">
      <TextInput
        ref="input"
        v-model="name"
        placeholder="my-service"
        spellcheck="false"
        :disabled="busy"
      />
    </Field>
    <div class="segmented" role="group" aria-label="Where">
      <Button
        :aria-pressed="!onRemote"
        :class="{ active: !onRemote }"
        :disabled="busy"
        @click="onRemote = false"
      >
        Local only
      </Button>
      <Button
        :aria-pressed="onRemote"
        :class="{ active: onRemote }"
        :disabled="busy || !status?.authenticated"
        @click="onRemote = true"
      >
        On {{ forgeName }}
      </Button>
    </div>
    <Field v-if="forges.length > 1" label="Forge">
      <Select v-model="forge" :options="forgeOptions" :disabled="busy" />
    </Field>
    <p v-if="status && !status.authenticated" class="muted">
      {{ status.error ?? `${forgeName} is not available` }}
    </p>
    <template v-if="onRemote">
      <Field label="Owner">
        <Select v-model="owner" :options="ownerOptions" :disabled="busy" />
      </Field>
      <Field label="Visibility">
        <Select v-model="visibility" :options="visibilityOptions" :disabled="busy" />
      </Field>
    </template>
    <p v-if="preview" class="muted mono">→ {{ preview }}</p>
    <p v-if="error" class="error">{{ error }}</p>
    <div class="actions">
      <Button :disabled="busy" @click="emit('close')">Cancel</Button>
      <Button type="submit" :disabled="busy || !!nameError">
        {{ busy ? 'Creating…' : 'Create' }}<Kbd v-if="!busy" action="confirm" />
      </Button>
    </div>
  </form>
</template>

<style scoped>
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
