<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { api, errorMessage } from '../api';
import { leaveSettings } from '../settingsGuard';
import { addSession, addTask, state } from '../store';

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
    });
    addTask(state, created.task);
    state.selectedTaskId = created.task.id;
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
        <button type="submit" :disabled="busy">Create</button>
      </div>
    </form>
  </div>
</template>
