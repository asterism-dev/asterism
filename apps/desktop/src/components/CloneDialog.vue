<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { sourceOwnerRepo, targetPath } from '../projects';
import { state, toast } from '../store';
import type { GithubRepo, GithubStatus } from '../types';

const emit = defineEmits<{ close: [] }>();

const source = ref('');
const status = ref<GithubStatus | null>(null);
const owner = ref('');
const repos = ref<GithubRepo[]>([]);
const filter = ref('');
const reposRoot = ref('');
const error = ref<string | null>(null);
const busy = ref(false);
const input = ref<HTMLInputElement>();

const owners = computed(() => (status.value?.login ? [status.value.login, ...status.value.orgs] : []));
const parsed = computed(() => sourceOwnerRepo(source.value));
const preview = computed(() => (parsed.value && reposRoot.value ? targetPath(reposRoot.value, parsed.value.owner, parsed.value.repo) : ''));
const shown = computed(() => {
  const needle = filter.value.trim().toLowerCase();
  return repos.value.filter((r) => !needle || r.name_with_owner.toLowerCase().includes(needle));
});

async function loadRepos() {
  repos.value = [];
  error.value = null;
  const o = owner.value;
  if (!o) return;
  try {
    const list = await api.githubRepos(o);
    if (owner.value === o) repos.value = list;
  } catch (e) {
    if (owner.value === o) error.value = errorMessage(e);
  }
}

onMounted(async () => {
  input.value?.focus();
  try {
    const [gh, config] = await Promise.all([api.githubStatus(), api.nodeConfig()]);
    status.value = gh;
    reposRoot.value = config.config.paths.repos;
    owner.value = gh.login ?? '';
  } catch (e) {
    error.value = errorMessage(e);
  }
});

watch(owner, loadRepos);

async function clone() {
  if (!parsed.value) {
    error.value = 'Enter owner/repo or a git URL.';
    return;
  }
  busy.value = true;
  error.value = null;
  try {
    const project = await api.cloneProject(source.value.trim());
    delete state.collapsed[project.id];
    toast(`Cloned ${parsed.value.owner}/${parsed.value.repo}`);
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
</script>

<template>
  <div class="modal-backdrop" tabindex="-1" @click.self="!busy && emit('close')" @keydown.esc="!busy && emit('close')">
    <form class="modal" @submit.prevent="clone">
      <h2>Clone repository</h2>
      <label>Repository <input ref="input" v-model="source" placeholder="owner/repo or https://… / git@…" spellcheck="false" :disabled="busy" /></label>
      <p v-if="preview" class="muted mono">→ {{ preview }}</p>
      <template v-if="status?.logged_in">
        <label>Browse
          <select v-model="owner" :disabled="busy">
            <option v-for="o in owners" :key="o" :value="o">{{ o }}</option>
          </select>
        </label>
        <input v-model="filter" placeholder="Filter repositories" aria-label="Filter repositories" :disabled="busy" @keydown.enter.prevent />
        <ul class="repo-list">
          <li v-for="r in shown" :key="r.name_with_owner">
            <button type="button" :disabled="busy" @click="source = r.name_with_owner">
              {{ r.name_with_owner }} <span v-if="r.private" class="muted">private</span>
              <span v-if="r.description" class="muted">— {{ r.description }}</span>
            </button>
          </li>
        </ul>
      </template>
      <p v-else-if="status" class="muted">{{ status.error ?? 'GitHub CLI not available' }} — you can still clone by URL.</p>
      <p v-if="error" class="error">{{ error }}</p>
      <div class="actions">
        <button type="button" :disabled="busy" @click="emit('close')">Cancel</button>
        <button type="submit" :disabled="busy">{{ busy ? 'Cloning…' : 'Clone' }}</button>
      </div>
    </form>
  </div>
</template>
