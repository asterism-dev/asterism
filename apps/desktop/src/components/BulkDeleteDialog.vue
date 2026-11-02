<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { api, errorMessage } from '../api';
import { deleteWarnings } from '../projectPage';
import { deleteTasks } from '../taskActions';
import type { Task } from '../types';

const props = defineProps<{ tasks: Task[] }>();
const emit = defineEmits<{ close: []; deleted: [] }>();

const warnings = ref<string[] | null>(null);
const error = ref<string | null>(null);
const deleteBranches = ref(false);
const busy = ref(false);
const confirmButton = ref<HTMLButtonElement>();

onMounted(async () => {
  confirmButton.value?.focus();
  try {
    warnings.value = deleteWarnings(
      await Promise.all(props.tasks.map((t) => api.deleteCheck(t.id))),
    );
  } catch (e) {
    error.value = errorMessage(e);
  }
});

function close() {
  if (!busy.value) emit('close');
}

async function confirm() {
  busy.value = true;
  await deleteTasks(props.tasks, deleteBranches.value);
  busy.value = false;
  emit('deleted');
}
</script>

<template>
  <div class="modal-backdrop" tabindex="-1" @click.self="close" @keydown.esc="close">
    <div class="modal" role="dialog" aria-modal="true" aria-label="Delete tasks">
      <h3>Delete {{ tasks.length }} task(s)?</h3>
      <p v-if="error" class="error">{{ error }}</p>
      <p v-else-if="!warnings" class="muted">Checking worktrees…</p>
      <ul v-else-if="warnings.length">
        <li v-for="w in warnings" :key="w">{{ w }}</li>
      </ul>
      <label
        ><input v-model="deleteBranches" type="checkbox" :disabled="busy" /> Delete branches</label
      >
      <div class="actions">
        <button type="button" :disabled="busy" @click="close">Cancel</button>
        <button
          ref="confirmButton"
          type="button"
          class="danger"
          :disabled="busy || !warnings"
          @click="confirm"
        >
          {{ busy ? 'Deleting…' : 'Delete' }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
h3 {
  margin: 0;
}
ul {
  margin: 0;
  padding-left: 18px;
}
button.danger {
  color: var(--danger);
}
</style>
