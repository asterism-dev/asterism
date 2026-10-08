<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { sourceOwnerRepo, targetPath } from '../projects';
import { state, toast } from '../store';
import Kbd from './Kbd.vue';
import type { ForgeInfo, ForgeRepo, ForgeStatus } from '../types';

const emit = defineEmits<{ close: []; busy: [boolean] }>();

const source = ref('');
const forges = ref<ForgeInfo[]>([]);
const forge = ref('');
const status = ref<ForgeStatus | null>(null);
const owner = ref('');
const repos = ref<ForgeRepo[]>([]);
const filter = ref('');
const reposRoot = ref('');
const error = ref<string | null>(null);
const busy = ref(false);
const input = ref<HTMLInputElement>();

const owners = computed(() =>
  status.value?.account ? [status.value.account, ...status.value.owners] : [],
);
const forgeName = computed(
  () => forges.value.find((f) => f.id === forge.value)?.display_name ?? 'remote',
);
const parsed = computed(() => sourceOwnerRepo(source.value));
const preview = computed(() =>
  parsed.value && reposRoot.value
    ? targetPath(reposRoot.value, parsed.value.owner, parsed.value.repo)
    : '',
);
const shown = computed(() => {
  const needle = filter.value.trim().toLowerCase();
  return repos.value.filter(
    (r) => !needle || `${r.owner}/${r.name}`.toLowerCase().includes(needle),
  );
});

async function loadRepos() {
  repos.value = [];
  error.value = null;
  const [f, o] = [forge.value, owner.value];
  if (!f || !o) return;
  try {
    const list = await api.forgeRepos(f, o);
    if (forge.value === f && owner.value === o) repos.value = list;
  } catch (e) {
    if (forge.value === f && owner.value === o) error.value = errorMessage(e);
  }
}

async function loadStatus() {
  status.value = null;
  owner.value = '';
  repos.value = [];
  const f = forge.value;
  if (!f) return;
  try {
    const next = await api.forgeStatus(f);
    if (forge.value !== f) return;
    status.value = next;
    owner.value = next.account ?? '';
  } catch (e) {
    if (forge.value === f) error.value = errorMessage(e);
  }
}

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

watch(forge, loadStatus);
watch(owner, loadRepos);

async function clone() {
  if (!parsed.value) {
    error.value = 'Enter owner/repo or a git URL.';
    return;
  }
  busy.value = true;
  error.value = null;
  try {
    const project = await api.cloneProject(source.value.trim(), forge.value || undefined);
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
watch(busy, (b) => emit('busy', b));
</script>

<template>
  <form class="add-form" @submit.prevent="clone">
    <label
      >Repository
      <input
        ref="input"
        v-model="source"
        placeholder="owner/repo or https://… / git@…"
        spellcheck="false"
        :disabled="busy"
    /></label>
    <p v-if="preview" class="muted mono">→ {{ preview }}</p>
    <label v-if="forges.length > 1"
      >Forge
      <select v-model="forge" :disabled="busy">
        <option v-for="f in forges" :key="f.id" :value="f.id">{{ f.display_name }}</option>
      </select>
    </label>
    <template v-if="status?.authenticated">
      <label
        >Browse
        <select v-model="owner" :disabled="busy">
          <option v-for="o in owners" :key="o" :value="o">{{ o }}</option>
        </select>
      </label>
      <input
        v-model="filter"
        placeholder="Filter repositories"
        aria-label="Filter repositories"
        :disabled="busy"
        @keydown.enter.prevent
      />
      <ul class="repo-list">
        <li v-for="r in shown" :key="`${r.owner}/${r.name}`">
          <button type="button" :disabled="busy" @click="source = `${r.owner}/${r.name}`">
            {{ r.owner }}/{{ r.name }} <span v-if="r.private" class="muted">private</span>
            <span v-if="r.description" class="muted">— {{ r.description }}</span>
          </button>
        </li>
      </ul>
    </template>
    <p v-else-if="status" class="muted">
      {{ status.error ?? `${forgeName} is not available` }} — you can still clone by URL.
    </p>
    <p v-if="error" class="error">{{ error }}</p>
    <div class="actions">
      <button type="button" :disabled="busy" @click="emit('close')">Cancel</button>
      <button type="submit" :disabled="busy">
        {{ busy ? 'Cloning…' : 'Clone' }}<Kbd v-if="!busy" action="confirm" />
      </button>
    </div>
  </form>
</template>
