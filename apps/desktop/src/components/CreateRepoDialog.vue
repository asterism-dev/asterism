<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { repoNameError, targetPath, visibilityChoices } from '../projects';
import { state, toast } from '../store';
import type { GithubStatus, Visibility } from '../types';

const emit = defineEmits<{ close: [] }>();

const name = ref('');
const onGithub = ref(false);
const status = ref<GithubStatus | null>(null);
const owner = ref('');
const visibility = ref<Visibility>('private');
const reposRoot = ref('');
const error = ref<string | null>(null);
const busy = ref(false);
const input = ref<HTMLInputElement>();

const owners = computed(() => (status.value?.login ? [status.value.login, ...status.value.orgs] : []));
const choices = computed(() => (status.value ? visibilityChoices(owner.value, status.value) : []));
const nameError = computed(() => (name.value ? repoNameError(name.value) : null));
const preview = computed(() =>
  name.value && !nameError.value && reposRoot.value
    ? targetPath(reposRoot.value, onGithub.value ? owner.value : 'local', name.value)
    : '',
);

watch(choices, (list) => {
  if (!list.includes(visibility.value)) visibility.value = 'private';
});

onMounted(async () => {
  input.value?.focus();
  try {
    const [gh, config] = await Promise.all([api.githubStatus(), api.nodeConfig()]);
    status.value = gh;
    owner.value = gh.login ?? '';
    reposRoot.value = config.config.paths.repos;
  } catch (e) {
    error.value = errorMessage(e);
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
    const github = onGithub.value ? { owner: owner.value, visibility: visibility.value } : null;
    const result = await api.createProject(name.value, github);
    delete state.collapsed[result.project.id];
    if (result.github_error) toast(`Created locally; GitHub failed: ${result.github_error}`);
    else toast(`Created ${name.value}`);
    emit('close');
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="modal-backdrop" @click.self="!busy && emit('close')" @keydown.esc="!busy && emit('close')">
    <form class="modal" @submit.prevent="create">
      <h2>New repository</h2>
      <label>Name <input ref="input" v-model="name" placeholder="my-service" spellcheck="false" :disabled="busy" /></label>
      <p v-if="nameError" class="error">{{ nameError }}</p>
      <div class="segmented" role="radiogroup" aria-label="Where">
        <button type="button" role="radio" :aria-checked="!onGithub" :class="{ active: !onGithub }" :disabled="busy" @click="onGithub = false">Local only</button>
        <button
          type="button"
          role="radio"
          :aria-checked="onGithub"
          :class="{ active: onGithub }"
          :disabled="busy || !status?.logged_in"
          @click="onGithub = true"
        >
          On GitHub
        </button>
      </div>
      <p v-if="status && !status.logged_in" class="muted">GitHub needs the GitHub CLI: run <code>gh auth login</code>.</p>
      <template v-if="onGithub">
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
  </div>
</template>
