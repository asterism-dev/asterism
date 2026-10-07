<script setup lang="ts">
import { html as renderDiff } from 'diff2html';
import { ColorSchemeType } from 'diff2html/lib/types';
import 'diff2html/bundles/css/diff2html.min.css';
import { computed, onMounted, ref, watch } from 'vue';
import { api, errorMessage } from '../api';
import { state, taskStatus } from '../store';
import { activeTheme } from '../theme';
import type { Task } from '../types';

const props = defineProps<{ task: Task }>();

const patch = ref<string | null>(null);
const error = ref<string | null>(null);
const loading = ref(false);
const sideBySide = ref(false);

const MAX_DIFF_CHANGES = 2000;
const MAX_DIFF_LINE_LENGTH = 2000;

async function load() {
  loading.value = true;
  try {
    patch.value = (await api.diff(props.task.id)).patch;
    error.value = null;
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    loading.value = false;
  }
}

// diff2html escapes file contents, so its output is safe for v-html.
const rendered = computed(() =>
  patch.value
    ? renderDiff(patch.value, {
        outputFormat: sideBySide.value ? 'side-by-side' : 'line-by-line',
        drawFileList: true,
        diffMaxChanges: MAX_DIFF_CHANGES,
        diffMaxLineLength: MAX_DIFF_LINE_LENGTH,
        colorScheme: activeTheme.value === 'dark' ? ColorSchemeType.DARK : ColorSchemeType.LIGHT,
      })
    : '',
);

// ponytail: reloads when the task's sessions settle instead of watching the worktree; add a daemon fs-watch event if agents edit while idle.
watch(
  () => taskStatus(state, props.task.id),
  (status) => {
    if (status !== 'working') load();
  },
);

onMounted(load);
</script>

<template>
  <div class="diff-view">
    <div class="diff-toolbar">
      <button :disabled="loading" @click="load">Refresh</button>
      <label><input v-model="sideBySide" type="checkbox" /> Side by side</label>
    </div>
    <p v-if="error" class="error">{{ error }}</p>
    <p v-else-if="patch === ''" class="muted">No changes against {{ task.base_branch }}.</p>
    <!-- eslint-disable-next-line vue/no-v-html -- diff2html escapes file contents -->
    <div v-else class="diff" v-html="rendered"></div>
  </div>
</template>

<style scoped>
.diff-view {
  position: absolute;
  inset: 0;
  overflow: auto;
  padding: 10px 14px;
}
.diff-toolbar {
  display: flex;
  gap: 12px;
  align-items: center;
  margin-bottom: 10px;
}
.diff-toolbar label {
  display: flex;
  gap: 6px;
  align-items: center;
  width: auto;
}
.diff-toolbar input {
  width: auto;
}
</style>
