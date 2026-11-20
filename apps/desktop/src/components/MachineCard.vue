<script setup lang="ts">
import { ChevronRight } from '@lucide/vue';
import { machineStatusLine, machineSummary } from '../machines';
import { state } from '../store';
import type { Machine } from '../types';
import MachineDot from './MachineDot.vue';

defineProps<{ machine: Machine }>();
</script>

<template>
  <button type="button" class="card">
    <MachineDot :machine="machine" />
    <span class="body">
      <span class="top">
        <span>{{ machine.name }}</span>
        <span class="muted">{{ machine.destination }}</span>
        <span class="muted status">{{ machineStatusLine(machine) }}</span>
      </span>
      <span class="muted">{{ machineSummary(machine, state.tasks) }}</span>
    </span>
    <ChevronRight :size="14" />
  </button>
</template>

<style scoped>
.card {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: none;
  font: inherit;
  color: inherit;
  text-align: left;
  cursor: pointer;
}
.card:hover {
  background: var(--select);
}
.body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.top {
  display: flex;
  gap: 10px;
}
.status {
  margin-left: auto;
}
</style>
