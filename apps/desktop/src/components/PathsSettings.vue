<script setup lang="ts">
import { Button, Checkbox, NumberInput, TextInput } from '@asterism-dev/lumen';
import { open } from '@tauri-apps/plugin-dialog';
import { computed, onMounted, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { state, toast } from '../store';
import type { ArchiveSettings, NodeConfig, PathSettings } from '../types';

const drafts = ref<Record<number, PathSettings>>({});
const baseline = ref<Record<number, PathSettings>>({});
const globals = ref<PathSettings>({ repos: '', worktrees: '' });
const node = ref<NodeConfig | null>(null);
const archive = ref<ArchiveSettings>({
  auto_delete_after_days: null,
  delete_branch: false,
  force: false,
});
const archiveBaseline = ref('');
const loaded = ref(false);
const error = ref<string | null>(null);
const saving = ref(false);

const changed = computed(() =>
  state.workspaces.filter((w) => {
    const draft = drafts.value[w.id];
    const base = baseline.value[w.id];
    return draft && base && (draft.repos !== base.repos || draft.worktrees !== base.worktrees);
  }),
);
const archiveDirty = computed(() => JSON.stringify(archive.value) !== archiveBaseline.value);
const dirty = computed(() => changed.value.length > 0 || archiveDirty.value);
const autoDelete = computed({
  get: () => archive.value.auto_delete_after_days != null,
  set: (on: boolean) => (archive.value.auto_delete_after_days = on ? 14 : null),
});

watch(dirty, (value) => (state.settingsDirty = value), { immediate: true });
watch(() => state.workspaces, syncNew, { immediate: true, deep: true });

onMounted(load);

function syncNew() {
  for (const w of state.workspaces) {
    if (drafts.value[w.id]) continue;
    drafts.value[w.id] = { repos: w.repos, worktrees: w.worktrees };
    baseline.value[w.id] = { repos: w.repos, worktrees: w.worktrees };
  }
}

async function load() {
  error.value = null;
  try {
    const { config } = await api.nodeConfig();
    globals.value = config.paths;
    node.value = config;
    archive.value = {
      ...config.archive,
      auto_delete_after_days: config.archive.auto_delete_after_days ?? null,
    };
    archiveBaseline.value = JSON.stringify(archive.value);
    loaded.value = true;
  } catch (e) {
    error.value = errorMessage(e);
  }
}

async function choose(id: number, key: keyof PathSettings) {
  error.value = null;
  const picked = await open({
    directory: true,
    multiple: false,
    defaultPath: drafts.value[id][key] || globals.value[key] || undefined,
  });
  if (typeof picked === 'string') drafts.value[id][key] = picked;
}

async function save() {
  saving.value = true;
  error.value = null;
  for (const w of changed.value) {
    try {
      const { repos, worktrees } = drafts.value[w.id];
      await api.updateWorkspace({ ...w, repos, worktrees });
      baseline.value[w.id] = { repos, worktrees };
    } catch (e) {
      error.value = `${w.name}: ${errorMessage(e)}`;
      break;
    }
  }
  if (!error.value && archiveDirty.value && node.value) {
    try {
      await api.setNodeConfig({ ...node.value, archive: archive.value });
      archiveBaseline.value = JSON.stringify(archive.value);
    } catch (e) {
      error.value = errorMessage(e);
    }
  }
  saving.value = false;
  if (!error.value) toast('Saved settings');
}

const fields: { key: keyof PathSettings; label: string }[] = [
  { key: 'repos', label: 'Repositories' },
  { key: 'worktrees', label: 'Worktrees' },
];
</script>

<template>
  <div class="agent-settings">
    <section v-for="w in state.workspaces" :key="w.id">
      <h3>{{ w.name }}</h3>
      <template v-if="drafts[w.id]">
        <div v-for="f in fields" :key="f.key" class="settings-row">
          <label :for="`path-${w.id}-${f.key}`">{{ f.label }}</label>
          <TextInput
            :id="`path-${w.id}-${f.key}`"
            v-model="drafts[w.id][f.key]"
            :placeholder="globals[f.key]"
            spellcheck="false"
          />
          <Button @click="choose(w.id, f.key).catch((e) => (error = errorMessage(e)))">
            Choose…
          </Button>
          <span v-if="!drafts[w.id][f.key]" class="muted">global default</span>
        </div>
      </template>
    </section>
    <p class="muted">
      Repositories are placed in &lt;owner&gt;/&lt;repo&gt;; worktrees in
      &lt;owner&gt;/&lt;repo&gt;/&lt;task&gt;.
    </p>
    <section v-if="loaded">
      <h3>Archived tasks</h3>
      <div class="settings-row">
        <Checkbox v-model="autoDelete">Delete automatically after</Checkbox>
        <NumberInput
          :model-value="archive.auto_delete_after_days ?? 14"
          :disabled="!autoDelete"
          min="1"
          aria-label="Days"
          @update:model-value="(v) => v !== null && (archive.auto_delete_after_days = v)"
        />
        <span>days</span>
      </div>
      <Checkbox v-model="archive.delete_branch" :disabled="!autoDelete">
        Also delete the branch
      </Checkbox>
      <Checkbox v-model="archive.force" :disabled="!autoDelete">
        Delete even if unsaved work would be lost
      </Checkbox>
      <p class="muted">
        The worktree is always removed. Tasks with uncommitted changes are skipped, and branches
        with unmerged commits kept, unless the last option is on.
      </p>
    </section>
    <div class="save-bar">
      <Button :variant="dirty ? 'accent' : 'default'" :disabled="saving || !dirty" @click="save">
        Save
      </Button>
      <span class="muted"
        >Applies to new repositories and tasks; existing worktrees stay where they are.</span
      >
    </div>
    <p v-if="error" class="error">{{ error }}</p>
    <Button v-if="error && !loaded" @click="load">Retry</Button>
  </div>
</template>
