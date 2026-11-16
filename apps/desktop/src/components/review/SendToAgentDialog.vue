<script setup lang="ts">
import { Button, Dialog, Field, Select, Textarea } from '@asterism-dev/lumen';
import { computed, onMounted, ref } from 'vue';
import { api, errorMessage } from '../../api';
import { agentSessions, defaultTarget } from '../../review';
import { state } from '../../store';
import type { ReviewSource, ReviewThread } from '../../types';

const props = defineProps<{
  taskId: number;
  source: ReviewSource;
  threads: ReviewThread[];
  initialPrompt?: string;
}>();
const emit = defineEmits<{ close: [] }>();
const prompt = ref('');
const target = ref<number | 'new'>(defaultTarget(state.sessions, props.taskId) ?? 'new');
const agents = computed(() => state.agents.filter((a) => a.available));
const agent = ref(agents.value[0]?.name ?? '');
const error = ref<string | null>(null);
const busy = ref(false);

const sessions = computed(() => agentSessions(state.sessions, props.taskId));
const targetOptions = computed(() => [
  ...sessions.value.map((s) => ({
    value: s.id as number | 'new',
    label: `#${s.id} ${s.kind.type === 'agent' ? s.kind.name : ''} · ${s.status}`,
  })),
  { value: 'new' as const, label: 'New session' },
]);
const agentOptions = computed(() =>
  agents.value.map((a) => ({ value: a.name, label: a.display_name })),
);
const title = computed(() =>
  props.threads.length
    ? `Send ${props.threads.length} thread${props.threads.length === 1 ? '' : 's'} to an agent`
    : 'Send to an agent',
);
const busyWarning = computed(() => sessions.value.some((s) => s.status === 'working'));

onMounted(async () => {
  if (props.initialPrompt) {
    prompt.value = props.initialPrompt;
    return;
  }
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
  <Dialog :open="true" :title="title" @update:open="(o) => !o && close()">
    <Field label="Session">
      <Select v-model="target" :options="targetOptions" />
    </Field>
    <Field v-if="target === 'new'" label="Agent">
      <Select v-model="agent" :options="agentOptions" />
    </Field>
    <p v-if="busyWarning" class="hint">
      An agent in this task is working; parallel edits in one worktree can conflict.
    </p>
    <Textarea v-model="prompt" :rows="12" />
    <p v-if="error" class="error">{{ error }}</p>
    <template #actions>
      <Button :disabled="busy" @click="close">Cancel</Button>
      <Button
        variant="accent"
        :disabled="busy || !prompt.trim() || (target === 'new' && !agent)"
        @click="send"
      >
        Send
      </Button>
    </template>
  </Dialog>
</template>
