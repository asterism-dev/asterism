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
  emit('close');
}

function confirm() {
  void deleteTasks(props.tasks, deleteBranches.value);
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
    <Checkbox v-model="deleteBranches">Delete branches</Checkbox>
    <template #actions>
      <Button @click="close">Cancel</Button>
      <Button ref="confirmButton" variant="danger" :disabled="!warnings" @click="confirm"
        >Delete</Button
      >
    </template>
  </Dialog>
</template>

<style scoped>
ul {
  margin: 0;
  padding-left: 18px;
}
</style>
