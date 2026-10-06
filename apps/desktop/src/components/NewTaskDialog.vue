<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { baseChoice } from '../baseBranch';
import { leaveSettings } from '../settingsGuard';
import { addSession, addTask, state } from '../store';
import type { ProjectBranches } from '../types';

const props = defineProps<{ projectId: number }>();
const emit = defineEmits<{ close: [] }>();

const agents = computed(() => ('hello' in state.node ? state.node.hello.agents.filter((a) => a.available) : []));
const projectId = ref(props.projectId);
const title = ref('');
const prompt = ref('');
const agent = ref(agents.value[0]?.name ?? '');
const error = ref<string | null>(null);
const busy = ref(false);
const titleInput = ref<HTMLInputElement>();

const branches = ref<ProjectBranches | null>(null);
const loadingBranches = ref(false);
const base = ref('');
const choice = computed(() => baseChoice(branches.value));

async function loadBranches() {
  const id = projectId.value;
  branches.value = null;
  loadingBranches.value = true;
  try {
    const result = await api.projectBranches(id);
    if (id !== projectId.value) return;
    branches.value = result;
    base.value = baseChoice(result).selected;
  } catch (e) {
    if (id === projectId.value) error.value = errorMessage(e);
  } finally {
    if (id === projectId.value) loadingBranches.value = false;
  }
}

watch(projectId, loadBranches, { immediate: true });
onMounted(() => titleInput.value?.focus());

async function submit() {
  if (!title.value.trim()) {
    error.value = 'Give the task a title.';
    return;
  }
  busy.value = true;
  try {
    if (!(await leaveSettings())) return;
    const created = await api.createTask({
      project_id: projectId.value,
      title: title.value.trim(),
      prompt: (agent.value && prompt.value.trim()) || null,
      agent: agent.value || null,
      base: base.value || null,
    });
    addTask(state, created.task);
    state.projectPage = null;
    state.selectedTaskId = created.task.id;
    delete state.collapsed[projectId.value];
    if (created.session) addSession(state, created.session);
    emit('close');
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('close')" @keydown.esc="emit('close')">
    <form class="modal" @submit.prevent="submit">
      <h2>New task</h2>
      <label>Project
        <select v-model="projectId">
          <option v-for="p in state.projects" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
      </label>
      <label>Base branch
        <select v-model="base" :disabled="loadingBranches || !choice.canCreate">
          <option v-if="loadingBranches" value="">Fetching…</option>
          <option v-for="b in choice.options" :key="b" :value="b">{{ b }}</option>
        </select>
      </label>
      <p v-if="choice.hint" class="muted">{{ choice.hint }}</p>
      <label>Title <input ref="titleInput" v-model="title" placeholder="Fix the login redirect" /></label>
      <label>Prompt <textarea
        v-model="prompt"
        rows="5"
        :disabled="!agent"
        :placeholder="agent ? 'Optional — sent to the agent on start' : 'Choose an agent to send a prompt'"
      /></label>
      <label>Agent
        <select v-model="agent">
          <option v-for="a in agents" :key="a.name" :value="a.name">{{ a.name }}</option>
          <option value="">No agent (empty worktree)</option>
        </select>
      </label>
      <p v-if="error" class="error">{{ error }}</p>
      <div class="actions">
        <button type="button" @click="emit('close')">Cancel</button>
        <button type="submit" :disabled="busy || loadingBranches || !choice.canCreate">Create</button>
      </div>
    </form>
  </div>
</template>
