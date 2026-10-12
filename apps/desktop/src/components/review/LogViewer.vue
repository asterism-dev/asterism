<script setup lang="ts">
import { openUrl } from '@tauri-apps/plugin-opener';
import { computed, ref } from 'vue';
import { errorMessage } from '../../api';
import { logExcerpt, logSections } from '../../review';
import { toast } from '../../store';
import type { CheckLog } from '../../types';

const props = defineProps<{ log: CheckLog }>();
const emit = defineEmits<{ toAgent: [excerpt: string] }>();
const sections = computed(() => logSections(props.log.text));
const anyFailed = computed(() => sections.value.some((s) => s.failed));
// Failed steps start open; without failures only the last step does.
const open = ref(
  new Set(
    sections.value.flatMap((s, i) =>
      s.failed || (!anyFailed.value && i === sections.value.length - 1) ? [i] : [],
    ),
  ),
);

function toggle(i: number) {
  const next = new Set(open.value);
  if (next.has(i)) next.delete(i);
  else next.add(i);
  open.value = next;
}

function openExternal(url: string) {
  openUrl(url).catch((e) => toast(errorMessage(e)));
}
</script>

<template>
  <div class="log">
    <div class="bar">
      <span v-if="log.truncated" class="muted">Log truncated to the last 1 MB.</span>
      <span class="spacer" />
      <button @click="emit('toAgent', logExcerpt(sections))">→ Agent</button>
      <button @click="openExternal(log.url)">Open in browser ↗</button>
    </div>
    <div v-for="(s, i) in sections" :key="i" class="section" :class="{ failed: s.failed }">
      <button v-if="s.title !== null" class="step" @click="toggle(i)">
        {{ open.has(i) ? '▾' : '▸' }} {{ s.title }}
      </button>
      <!-- ponytail: renders every open line; virtualize if multi-MB logs feel slow. -->
      <pre v-if="s.title === null || open.has(i)">{{ s.lines.join('\n') }}</pre>
    </div>
  </div>
</template>

<style scoped>
.log {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: auto;
  font-size: 12px;
}
.bar {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 6px 0;
  position: sticky;
  top: 0;
  background: var(--bg);
}
.spacer {
  flex: 1;
}
.step {
  width: 100%;
  text-align: left;
  background: none;
  border: none;
  padding: 4px 0;
  font-weight: 600;
  cursor: pointer;
}
.failed .step {
  color: var(--danger);
}
pre {
  margin: 0 0 6px;
  padding: 6px 8px;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 4px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
</style>
