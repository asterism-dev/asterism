<script setup lang="ts">
import { computed, ref } from 'vue';
import { TriangleAlert } from '@lucide/vue';
import { api, errorMessage } from '../api';
import { floatKey, workspaceKey } from '../dock/model';
import { remove } from '../dock/storage';
import type { TaskDeleteCheck, WorktreeTask } from '../types';
import { replaceBlocked, type ConflictChoice, type WorktreeConflict } from '../worktreeConflict';

const props = defineProps<{ conflict: WorktreeConflict }>();
const choice = defineModel<ConflictChoice>({ required: true });
const emit = defineEmits<{ deleted: [] }>();

const blocked = computed(() => replaceBlocked(props.conflict));
const pending = ref<WorktreeTask | null>(null);
const check = ref<TaskDeleteCheck | null>(null);
const deleteBranch = ref(false);
const error = ref<string | null>(null);

const label = (c: ConflictChoice) =>
  c === 'reuse'
    ? 'Reuse'
    : c === 'suffix'
      ? `Create additional (${props.conflict.suffixed ?? '—'})`
      : 'Replace';

async function startDelete(task: WorktreeTask) {
  pending.value = task;
  check.value = null;
  deleteBranch.value = false;
  error.value = null;
  try {
    check.value = await api.deleteCheck(task.id);
  } catch (e) {
    error.value = errorMessage(e);
  }
}

async function confirmDelete() {
  const task = pending.value;
  if (!task) return;
  try {
    await api.deleteTask(task.id, deleteBranch.value);
    remove(workspaceKey(task.id));
    remove(floatKey(task.id));
    pending.value = null;
    emit('deleted');
  } catch (e) {
    error.value = errorMessage(e);
  }
}
</script>

<template>
  <div class="conflict">
    <p class="title">
      <TriangleAlert :size="14" />
      {{ conflict.worktree ? 'Worktree' : 'Branch' }}
      <span class="mono">{{ conflict.name }}</span> already exists
    </p>
    <div class="segmented small" role="group" aria-label="Existing worktree">
      <button
        v-for="c in conflict.choices"
        :key="c"
        type="button"
        :class="{ active: choice === c }"
        :aria-pressed="choice === c"
        :disabled="c === 'replace' && blocked"
        :title="c === 'replace' && blocked ? 'Delete the task using it first' : undefined"
        @click="choice = c"
      >
        {{ label(c) }}
      </button>
    </div>
    <p v-if="choice === 'replace'" class="warning">
      Discards uncommitted changes and local commits in {{ conflict.name }}.
    </p>
    <ul v-if="conflict.worktree?.tasks.length" class="owners">
      <li v-for="t in conflict.worktree.tasks" :key="t.id">
        <div class="row">
          <span
            >Belongs to <strong>{{ t.title }}</strong> ·
            {{ t.archived ? 'archived' : 'active' }}</span
          >
          <button v-if="pending?.id !== t.id" type="button" class="danger" @click="startDelete(t)">
            Delete task
          </button>
        </div>
        <div v-if="pending?.id === t.id" class="confirm">
          <p v-if="!check && !error" class="muted">Checking…</p>
          <template v-if="check">
            <p v-if="check.shared_with.length" class="muted">
              The worktree stays: {{ check.shared_with.length }} other task(s) use it.
            </p>
            <p v-else-if="check.dirty" class="warning">Uncommitted changes will be lost.</p>
            <label v-if="check.branch_exists && !check.shared_with.length" class="switch">
              <input v-model="deleteBranch" type="checkbox" role="switch" />
              Also delete branch <span class="mono">{{ check.branch }}</span>
              <span v-if="check.unmerged_commits" class="warning"
                >({{ check.unmerged_commits }} unmerged commit(s))</span
              >
            </label>
            <div class="row">
              <button type="button" @click="pending = null">Cancel</button>
              <button type="button" class="danger" @click="confirmDelete">Delete</button>
            </div>
          </template>
        </div>
      </li>
    </ul>
    <p v-if="error" class="error">{{ error }}</p>
  </div>
</template>

<style scoped>
.conflict {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 12px;
}
.conflict p {
  margin: 0;
}
.title {
  display: flex;
  align-items: center;
  gap: 6px;
}
.segmented.small button {
  border: none;
  padding: 2px 8px;
}
.warning,
button.danger {
  color: var(--danger);
}
.owners {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.confirm {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-left: 8px;
}
.switch {
  display: flex;
  align-items: center;
  gap: 6px;
}
.switch input {
  width: auto;
}
</style>
