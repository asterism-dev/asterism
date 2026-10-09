<script setup lang="ts">
import { ref } from 'vue';
import { errorMessage } from '../../api';
import type { CommentTarget } from '../../types';

const props = defineProps<{
  buttons: { label: string; target: CommentTarget }[];
  submit: (body: string, target: CommentTarget) => Promise<void>;
  placeholder?: string;
}>();
const emit = defineEmits<{ cancel: [] }>();
const body = ref('');
const busy = ref(false);
const error = ref<string | null>(null);

// A failed post keeps the text in the editor and shows the error.
async function run(target: CommentTarget) {
  busy.value = true;
  error.value = null;
  try {
    await props.submit(body.value, target);
    body.value = '';
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="comment-editor">
    <textarea v-model="body" rows="4" :placeholder="placeholder ?? 'Leave a comment'" />
    <p v-if="error" class="error">{{ error }}</p>
    <div class="actions">
      <button :disabled="busy" @click="emit('cancel')">Cancel</button>
      <button
        v-for="b in buttons"
        :key="b.target"
        :disabled="busy || !body.trim()"
        @click="run(b.target)"
      >
        {{ b.label }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.comment-editor {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px;
  white-space: normal;
}
.comment-editor textarea {
  resize: vertical;
}
.actions {
  display: flex;
  gap: 6px;
  justify-content: flex-end;
}
</style>
