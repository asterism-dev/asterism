<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { api, errorMessage } from '../../api';
import { agentSessions, defaultTarget } from '../../review';
import { state } from '../../store';
import type { ReviewSource, ReviewThread } from '../../types';

const props = defineProps<{ taskId: number; source: ReviewSource; threads: ReviewThread[] }>();
const emit = defineEmits<{ close: [] }>();
const prompt = ref('');
const target = ref<number | 'new'>(defaultTarget(state.sessions, props.taskId) ?? 'new');
const agents = computed(() => state.agents.filter((a) => a.available));
const agent = ref(agents.value[0]?.name ?? '');
const error = ref<string | null>(null);
const busy = ref(false);

const sessions = computed(() => agentSessions(state.sessions, props.taskId));
const busyWarning = computed(() => sessions.value.some((s) => s.status === 'working'));

onMounted(async () => {
  try {
    const ids = props.threads.map((t) => t.id);
    prompt.value = (await api.reviewPrompt(props.taskId, props.source, ids)).prompt;
  } catch (e) {
    error.value = errorMessage(e);
  }
});

function close() {
  if (!busy.value) emit('close');
}

async function send() {
  busy.value = true;
  error.value = null;
  try {
    if (target.value === 'new')
      await api.startSession(props.taskId, { type: 'agent', name: agent.value }, prompt.value);
    else await api.sendPrompt(target.value, prompt.value);
    busy.value = false;
    emit('close');
  } catch (e) {
    error.value = errorMessage(e);
    busy.value = false;
  }
}
</script>

<template>
  <div class="modal-backdrop" tabindex="-1" @click.self="close" @keydown.esc="close">
    <div class="modal" role="dialog" aria-modal="true" aria-label="Send to an agent">
      <strong>
        Send {{ threads.length }} thread{{ threads.length === 1 ? '' : 's' }} to an agent
      </strong>
      <label>
        Session
        <select v-model="target">
          <option v-for="s in sessions" :key="s.id" :value="s.id">
            #{{ s.id }} {{ s.kind.type === 'agent' ? s.kind.name : '' }} · {{ s.status }}
          </option>
          <option value="new">New session</option>
        </select>
      </label>
      <label v-if="target === 'new'">
        Agent
        <select v-model="agent">
          <option v-for="a in agents" :key="a.name" :value="a.name">{{ a.display_name }}</option>
        </select>
      </label>
      <p v-if="busyWarning" class="hint">
        An agent in this task is working; parallel edits in one worktree can conflict.
      </p>
      <textarea v-model="prompt" rows="12" />
      <p v-if="error" class="error">{{ error }}</p>
      <div class="actions">
        <button type="button" :disabled="busy" @click="close">Cancel</button>
        <button
          type="button"
          class="primary"
          :disabled="busy || !prompt.trim() || (target === 'new' && !agent)"
          @click="send"
        >
          Send
        </button>
      </div>
    </div>
  </div>
</template>
