<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { repoNameError, targetPath, visibilityChoices } from '../projects';
import { state, toast } from '../store';
import type { ForgeInfo, ForgeStatus, Visibility } from '../types';

const emit = defineEmits<{ close: []; busy: [boolean] }>();

const name = ref('');
const onRemote = ref(false);
const forges = ref<ForgeInfo[]>([]);
const forge = ref('');
const status = ref<ForgeStatus | null>(null);
const owner = ref('');
const visibility = ref<Visibility>('private');
const reposRoot = ref('');
const error = ref<string | null>(null);
const busy = ref(false);
const input = ref<HTMLInputElement>();

const owners = computed(() => (status.value?.account ? [status.value.account, ...status.value.owners] : []));
const forgeName = computed(() => forges.value.find((f) => f.id === forge.value)?.display_name ?? 'remote');
const choices = computed(() => (status.value ? visibilityChoices(owner.value, status.value) : []));
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
  input.value?.focus();
  try {
    const [list, config] = await Promise.all([api.forges(), api.nodeConfig()]);
    forges.value = list;
    reposRoot.value = config.config.paths.repos;
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
    const remote = onRemote.value ? { forge: forge.value, owner: owner.value, visibility: visibility.value } : null;
    const result = await api.createProject(name.value, remote);
    delete state.collapsed[result.project.id];
    if (result.remote_error) toast(`Created locally; ${forgeName.value} failed: ${result.remote_error}`);
    else toast(`Created ${name.value}`);
    emit('close');
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
    if (error.value) {
      await nextTick();
      input.value?.focus();
    }
  }
}
watch(busy, (b) => emit('busy', b));
</script>

<template>
  <form class="add-form" @submit.prevent="create">
    <label>Name <input ref="input" v-model="name" placeholder="my-service" spellcheck="false" :disabled="busy" /></label>
    <p v-if="nameError" class="error">{{ nameError }}</p>
    <div class="segmented" role="group" aria-label="Where">
      <button type="button" :aria-pressed="!onRemote" :class="{ active: !onRemote }" :disabled="busy" @click="onRemote = false">Local only</button>
      <button
        type="button"
        :aria-pressed="onRemote"
        :class="{ active: onRemote }"
        :disabled="busy || !status?.authenticated"
        @click="onRemote = true"
      >
        On {{ forgeName }}
      </button>
    </div>
    <label v-if="forges.length > 1">Forge
      <select v-model="forge" :disabled="busy">
        <option v-for="f in forges" :key="f.id" :value="f.id">{{ f.display_name }}</option>
      </select>
    </label>
    <p v-if="status && !status.authenticated" class="muted">{{ status.error ?? `${forgeName} is not available` }}</p>
    <template v-if="onRemote">
      <label>Owner
        <select v-model="owner" :disabled="busy">
          <option v-for="o in owners" :key="o" :value="o">{{ o }}</option>
        </select>
      </label>
      <label>Visibility
        <select v-model="visibility" :disabled="busy">
          <option v-for="v in choices" :key="v" :value="v">{{ v }}</option>
        </select>
      </label>
    </template>
    <p v-if="preview" class="muted mono">→ {{ preview }}</p>
    <p v-if="error" class="error">{{ error }}</p>
    <div class="actions">
      <button type="button" :disabled="busy" @click="emit('close')">Cancel</button>
      <button type="submit" :disabled="busy || !!nameError">{{ busy ? 'Creating…' : 'Create' }}</button>
    </div>
  </form>
</template>
