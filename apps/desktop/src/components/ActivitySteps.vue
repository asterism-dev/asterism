<script setup lang="ts">
import { Check, LoaderCircle, X } from '@lucide/vue';
import type { ActivityStep } from '../types';

defineProps<{ steps: ActivityStep[] }>();
</script>

<template>
  <ol class="steps">
    <li v-for="(step, i) in steps" :key="i" :class="step.status">
      <span class="mark">
        <LoaderCircle v-if="step.status === 'running'" class="spin" />
        <Check v-else-if="step.status === 'done'" />
        <X v-else-if="step.status === 'failed'" />
        <span v-else class="dot" />
      </span>
      <span class="label">{{ step.label }}</span>
      <span v-if="step.status === 'failed' && step.detail" class="detail">{{ step.detail }}</span>
    </li>
  </ol>
</template>

<style scoped>
.steps {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
}
li {
  display: grid;
  grid-template-columns: 16px 1fr;
  column-gap: 6px;
  align-items: center;
}
.mark {
  display: flex;
  align-items: center;
  justify-content: center;
}
.mark .lucide {
  width: 14px;
  height: 14px;
}
.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--muted);
}
.pending .label {
  color: var(--muted);
}
.done .mark {
  color: var(--muted);
}
.failed .mark {
  color: var(--danger);
}
.detail {
  grid-column: 2;
  color: var(--danger);
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
