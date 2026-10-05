<script setup lang="ts">
import type { SessionStatus } from '../types';

defineProps<{ status: SessionStatus | null }>();

const LABELS: Partial<Record<SessionStatus, string>> = {
  waiting_input: 'Waiting for input',
  working: 'Working',
  idle: 'Idle',
};
</script>

<template>
  <span v-if="status && LABELS[status]" class="dot" :class="status" :title="LABELS[status]" role="img" :aria-label="LABELS[status]"></span>
</template>
