<script setup lang="ts">
import { Button, Checkbox } from '@asterism-dev/lumen';
import { ref } from 'vue';
import { api, errorMessage } from '../../api';
import { formatTime } from '../../review';
import type { ReviewThread } from '../../types';
import CommentEditor from './CommentEditor.vue';

const props = defineProps<{
  taskId: number;
  thread: ReviewThread;
  canPublish: boolean;
  pendingReview: boolean;
  selected: boolean;
}>();
const emit = defineEmits<{ toAgent: [thread: ReviewThread]; select: [selected: boolean] }>();
const open = ref(!props.thread.resolved);
const replying = ref(false);
const error = ref<string | null>(null);

async function act(action: () => Promise<unknown>) {
  error.value = null;
  try {
    await action();
  } catch (e) {
    error.value = errorMessage(e);
  }
}

const toggleResolved = () =>
  act(() => api.reviewResolve(props.taskId, props.thread.id, !props.thread.resolved));
const publish = () =>
  act(() =>
    api.reviewPublish(props.taskId, props.thread.id, props.pendingReview ? 'review' : 'single'),
  );

async function reply(body: string) {
  await api.reviewReply(props.taskId, props.thread.id, body);
  replying.value = false;
}
</script>

<template>
  <div class="thread" :class="{ resolved: thread.resolved }">
    <div class="thread-head">
      <Checkbox
        :model-value="selected"
        title="Send to agent"
        @update:model-value="(v) => emit('select', v === true)"
      />
      <Button variant="ghost" size="sm" @click="open = !open">
        {{ thread.path }}:{{ thread.line }}
      </Button>
      <span v-if="thread.local" class="badge">local</span>
      <span v-if="thread.pending" class="badge">pending</span>
      <span v-if="thread.outdated" class="badge">outdated</span>
      <span v-if="thread.resolved" class="badge">resolved</span>
    </div>
    <template v-if="open">
      <div v-for="c in thread.comments" :key="c.id" class="comment">
        <strong>{{ thread.local ? 'local' : '@' + c.author }}</strong>
        <span class="muted">{{ formatTime(c.created_at) }}</span>
        <p class="body">{{ c.body }}</p>
      </div>
      <p v-if="error" class="error">{{ error }}</p>
      <div class="thread-actions">
        <Button variant="ghost" size="sm" @click="replying = !replying">Reply</Button>
        <Button variant="ghost" size="sm" @click="toggleResolved">
          {{ thread.resolved ? 'Unresolve' : 'Resolve' }}
        </Button>
        <Button v-if="thread.local && canPublish" variant="ghost" size="sm" @click="publish">
          Publish
        </Button>
        <Button variant="ghost" size="sm" @click="emit('toAgent', thread)">→ Agent</Button>
      </div>
      <CommentEditor
        v-if="replying"
        :buttons="[{ label: 'Reply', target: thread.local ? 'local' : 'single' }]"
        :submit="reply"
        @cancel="replying = false"
      />
    </template>
  </div>
</template>

<style scoped>
.thread {
  border: 1px solid var(--border);
  border-radius: 6px;
  margin: 6px 8px;
  padding: 6px 8px;
  background: var(--panel);
  white-space: normal;
}
.thread.resolved {
  opacity: 0.7;
}
.thread-head,
.thread-actions {
  display: flex;
  gap: 6px;
  align-items: center;
}
.badge {
  font-size: 11px;
  padding: 0 6px;
  border-radius: 8px;
  border: 1px solid var(--border);
}
.body {
  white-space: pre-wrap;
  margin: 2px 0 6px;
}
</style>
