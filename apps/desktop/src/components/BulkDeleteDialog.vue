<script setup lang="ts">
import { Button, Checkbox, Dialog } from '@asterism-dev/lumen';
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
const confirmButton = ref<InstanceType<typeof Button>>();

onMounted(async () => {
  confirmButton.value?.$el.focus();
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
  <Dialog
    :open="true"
    :title="`Delete ${tasks.length} task(s)?`"
    @update:open="(o) => !o && close()"
  >
    <p v-if="error" class="error">{{ error }}</p>
    <p v-else-if="!warnings" class="muted">Checking worktrees…</p>
    <ul v-else-if="warnings.length">
      <li v-for="w in warnings" :key="w">{{ w }}</li>
    </ul>
    <Checkbox v-model="deleteBranches" :disabled="busy">Delete branches</Checkbox>
    <template #actions>
      <Button :disabled="busy" @click="close">Cancel</Button>
      <Button ref="confirmButton" variant="danger" :disabled="busy || !warnings" @click="confirm">
        {{ busy ? 'Deleting…' : 'Delete' }}
      </Button>
    </template>
  </Dialog>
</template>

<style scoped>
ul {
  margin: 0;
  padding-left: 18px;
}
</style>
