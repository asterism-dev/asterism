<script setup lang="ts">
import type { Machine } from '../types';

defineProps<{ machine: Machine }>();
</script>

<template>
  <span class="dot" :class="machine.status.state" />
</template>

<style scoped>
.dot {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--muted);
}
.dot.ready {
  background: var(--success);
}
.dot.blocked {
  background: var(--danger);
}
.dot.connecting {
  background: var(--warning);
}
</style>
