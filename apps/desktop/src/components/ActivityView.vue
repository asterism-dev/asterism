<script setup lang="ts">
import { Table, type TableColumn } from '@asterism-dev/lumen';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { api, errorMessage } from '../api';
import { formatBytes, formatCpu } from '../stats';
import { isConnected, selectSession, sessionLabel, state } from '../store';
import type { NodeStats, ProcStats, Session } from '../types';
import StatusIndicator from './StatusIndicator.vue';

const POLL_MS = 3000;

interface Row {
  key: string;
  name: string;
  detail: string;
  stats: ProcStats;
  session?: Session;
}

const COLUMNS: TableColumn[] = [
  { key: 'name', label: 'Process' },
  { key: 'detail', label: 'Task', class: 'muted' },
  { key: 'memory', label: 'Memory', align: 'right', class: 'num' },
  { key: 'cpu', label: 'CPU', align: 'right', class: 'num' },
];

const stats = ref<NodeStats | null>(null);
const error = ref<string | null>(null);
let appPid: number | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
let stopped = false;

const rows = computed<Row[]>(() => {
  if (!stats.value) return [];
  const sessions = stats.value.sessions.flatMap(({ session_id, stats: s }) => {
    const session = state.sessions.find((x) => x.id === session_id);
    if (!session) return [];
    const task = state.tasks.find((t) => t.id === session.task_id);
    return [
      {
        key: `s${session_id}`,
        name: sessionLabel(session),
        detail: task?.title ?? '',
        stats: s,
        session,
      },
    ];
  });
  const app = stats.value.processes.find((p) => p.pid === appPid);
  return [
    ...(app ? [{ key: 'app', name: 'asterism', detail: 'Desktop app', stats: app.stats }] : []),
    { key: 'daemon', name: 'asterismd', detail: 'Daemon', stats: stats.value.daemon },
    ...sessions.sort((a, b) => b.stats.memory_bytes - a.stats.memory_bytes),
  ];
});
const total = computed(() =>
  rows.value.reduce(
    (sum, r) => ({
      memory_bytes: sum.memory_bytes + r.stats.memory_bytes,
      cpu_percent: sum.cpu_percent + r.stats.cpu_percent,
    }),
    { memory_bytes: 0, cpu_percent: 0 },
  ),
);

async function poll() {
  if (!document.hidden && isConnected(state.node)) {
    try {
      appPid ??= await api.appPid();
      stats.value = await api.nodeStats([appPid]);
      error.value = null;
    } catch (e) {
      error.value = errorMessage(e);
    }
  }
  if (!stopped) timer = setTimeout(poll, POLL_MS);
}

onMounted(poll);
onUnmounted(() => {
  stopped = true;
  clearTimeout(timer);
});
</script>

<template>
  <section class="activity-view">
    <h2 class="activity-title">Resources</h2>
    <div class="activity-body">
      <p v-if="error" class="error">{{ error }}</p>
      <p v-else-if="!stats" class="muted">Measuring…</p>
      <Table
        v-else
        :columns="COLUMNS"
        :rows="rows"
        row-key="key"
        size="sm"
        hover="clickable"
        :row-clickable="(r) => !!r.session"
        @row-click="(r) => r.session && selectSession(state, r.session)"
      >
        <template #cell-name="{ row: r }">
          <span class="process"
            >{{ r.name }}<StatusIndicator v-if="r.session" :status="r.session.status"
          /></span>
        </template>
        <template #cell-memory="{ row: r }">{{ formatBytes(r.stats.memory_bytes) }}</template>
        <template #cell-cpu="{ row: r }">{{ formatCpu(r.stats.cpu_percent) }}</template>
        <template #footer-name>Total</template>
        <template #footer-memory>{{ formatBytes(total.memory_bytes) }}</template>
        <template #footer-cpu>{{ formatCpu(total.cpu_percent) }}</template>
      </Table>
      <p class="muted note">
        Agents include their child processes (MCP servers, tools). CPU is relative to one core;
        WebKit's web process is not counted.
      </p>
    </div>
  </section>
</template>

<style scoped>
.activity-view {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
}
.activity-title {
  font-size: 13px;
  margin: 0;
  padding: 10px 14px 0;
}
.activity-body {
  overflow-y: auto;
  padding: 8px 14px 14px;
}
:deep(.num) {
  white-space: nowrap;
}
.process {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.note {
  font-size: 12px;
  margin-top: 12px;
}
</style>
