<script setup lang="ts">
import { openUrl } from '@tauri-apps/plugin-opener';
import { computed, ref } from 'vue';
import { api, errorMessage } from '../../api';
import { checkPrompt, failed, logExcerpt, logSections, timeValue } from '../../review';
import { toast } from '../../store';
import type { CheckLog, CheckRun } from '../../types';
import LogViewer from './LogViewer.vue';

const props = defineProps<{ taskId: number; checks: CheckRun[] }>();
const emit = defineEmits<{ toAgent: [prompt: string] }>();
const current = ref<CheckRun | null>(null);
const log = ref<CheckLog | null>(null);
const logError = ref<string | null>(null);
const rerunError = ref<Record<string, string>>({});
const busy = ref(new Set<string>());

const summary = computed(() => {
  const n = (f: (c: CheckRun) => boolean) => props.checks.filter(f).length;
  const bad = n(failed);
  const running = n((c) => c.status !== 'done');
  if (bad) return `${bad} failing`;
  if (running) return `${running} in progress`;
  return props.checks.length ? 'All checks passed' : 'No checks';
});

const icon = (c: CheckRun) =>
  c.status !== 'done' ? '●' : failed(c) ? '✗' : c.conclusion === 'success' ? '✓' : '–';
const tone = (c: CheckRun) =>
  c.status !== 'done'
    ? 'running'
    : failed(c)
      ? 'bad'
      : c.conclusion === 'success'
        ? 'ok'
        : 'neutral';

function duration(c: CheckRun): string {
  if (!c.started_at || !c.completed_at) return '';
  const s = Math.round((timeValue(c.completed_at) - timeValue(c.started_at)) / 1000);
  return s >= 60 ? `${Math.floor(s / 60)}m ${s % 60}s` : `${s}s`;
}

function openExternal(url: string) {
  openUrl(url).catch((e) => toast(errorMessage(e)));
}

async function show(c: CheckRun) {
  if (!c.has_log) return;
  current.value = c;
  log.value = null;
  logError.value =
    c.status !== 'done' ? 'Still running — the log is available when the job finishes.' : null;
  if (c.status !== 'done') return;
  try {
    const result = await api.reviewCheckLog(props.taskId, c.id);
    if (current.value?.id === c.id) log.value = result;
  } catch (e) {
    if (current.value?.id === c.id) logError.value = errorMessage(e);
  }
}

async function rerun(c: CheckRun) {
  busy.value = new Set(busy.value).add(c.id);
  rerunError.value = Object.fromEntries(
    Object.entries(rerunError.value).filter(([id]) => id !== c.id),
  );
  try {
    await api.reviewCheckRerun(props.taskId, c.id);
  } catch (e) {
    rerunError.value = { ...rerunError.value, [c.id]: errorMessage(e) };
  } finally {
    const next = new Set(busy.value);
    next.delete(c.id);
    busy.value = next;
  }
}

async function toAgent(c: CheckRun, excerpt?: string) {
  if (excerpt === undefined) {
    try {
      excerpt = logExcerpt(logSections((await api.reviewCheckLog(props.taskId, c.id)).text));
    } catch (e) {
      rerunError.value = { ...rerunError.value, [c.id]: errorMessage(e) };
      return;
    }
  }
  emit('toAgent', checkPrompt(c.workflow ? `${c.workflow} / ${c.name}` : c.name, excerpt));
}
</script>

<template>
  <div class="checks">
    <div class="list">
      <h4>{{ summary }}</h4>
      <div v-for="c in checks" :key="c.id" class="check" :class="{ on: current?.id === c.id }">
        <span class="icon" :class="tone(c)">{{ icon(c) }}</span>
        <button class="name" :disabled="!c.has_log" @click="show(c)">
          <span v-if="c.workflow" class="muted">{{ c.workflow }} / </span>{{ c.name }}
        </button>
        <span class="muted">{{ duration(c) }}</span>
        <button v-if="failed(c) && c.rerunnable" :disabled="busy.has(c.id)" @click="rerun(c)">
          Re-run
        </button>
        <button v-if="failed(c) && c.has_log" @click="toAgent(c)">→ Agent</button>
        <button class="link" title="Open in browser" @click="openExternal(c.url)">↗</button>
        <p v-if="rerunError[c.id]" class="error">{{ rerunError[c.id] }}</p>
      </div>
    </div>
    <div v-if="current" class="viewer">
      <h4>{{ current.name }}</h4>
      <p v-if="logError" class="muted">
        {{ logError }}
        <button class="link" @click="openExternal(current.url)">Open in browser ↗</button>
      </p>
      <p v-else-if="!log" class="muted">Loading log…</p>
      <LogViewer
        v-else
        :key="current.id"
        :log="log"
        @to-agent="(x) => current && toAgent(current, x)"
      />
    </div>
  </div>
</template>

<style scoped>
.checks {
  flex: 1;
  min-height: 0;
  display: flex;
}
.list {
  width: 420px;
  flex: none;
  overflow: auto;
  padding: 10px 14px;
  border-right: 1px solid var(--border);
}
.viewer {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  padding: 10px 14px;
  overflow: hidden;
}
h4 {
  margin: 0 0 8px;
  font-size: 13px;
}
.check {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  padding: 6px 4px;
  border-bottom: 1px solid var(--border);
  font-size: 12px;
}
.check.on {
  background: var(--select);
}
.name {
  flex: 1;
  text-align: left;
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.name:disabled {
  cursor: default;
}
.icon.ok {
  color: #2da44e;
}
.icon.bad {
  color: var(--danger);
}
.icon.running {
  color: var(--waiting);
}
.link {
  background: none;
  border: none;
  padding: 0 4px;
  cursor: pointer;
  color: var(--muted);
}
.error {
  width: 100%;
  margin: 0;
}
</style>
