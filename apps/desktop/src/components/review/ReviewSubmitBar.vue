<script setup lang="ts">
import { ref } from 'vue';
import { api, errorMessage } from '../../api';
import type { ReviewEvent, ReviewResult } from '../../types';

const props = defineProps<{ taskId: number; review: ReviewResult }>();
const open = ref(false);
const body = ref('');
const error = ref<string | null>(null);
const busy = ref(false);

async function submit(event: ReviewEvent) {
  busy.value = true;
  error.value = null;
  try {
    await api.reviewSubmit(props.taskId, event, body.value);
    open.value = false;
    body.value = '';
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="review-bar">
    <span v-if="review.pending_review" class="muted">
      Review: {{ review.pending_review.comments }} comment(s) pending
    </span>
    <span class="spacer" />
    <button @click="open = !open">
      {{ review.pending_review ? 'Finish review' : 'Review changes' }} ▾
    </button>
    <div v-if="open" class="submit-box">
      <textarea v-model="body" rows="3" placeholder="Leave a comment" />
      <p v-if="error" class="error">{{ error }}</p>
      <div class="actions">
        <button :disabled="busy" @click="submit('comment')">Comment</button>
        <button :disabled="busy" @click="submit('approve')">Approve</button>
        <button :disabled="busy" @click="submit('request_changes')">Request changes</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.review-bar {
  display: flex;
  gap: 10px;
  align-items: center;
  padding: 6px 14px;
  flex-wrap: wrap;
  border-bottom: 1px solid var(--border);
}
.spacer {
  flex: 1;
}
.submit-box {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.actions {
  display: flex;
  gap: 6px;
  justify-content: flex-end;
}
</style>
