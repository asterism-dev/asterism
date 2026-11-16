<script setup lang="ts">
import { Button, Tabs } from '@asterism-dev/lumen';
import { Archive, ChevronLeft } from '@lucide/vue';
import { computed } from 'vue';
import StatusIndicator from '../components/StatusIndicator.vue';
import { api, errorMessage } from '../api';
import { closeSession } from '../sessionActions';
import { addSession, sessionLabel, state, taskSessions, toast } from '../store';
import { archiveTask } from '../taskActions';
import type { Session, SessionKind } from '../types';
import MobileTerminal from './MobileTerminal.vue';
import type { DetailTab } from './route';
import { go } from './useRoute';

const props = defineProps<{ taskId: number; tab: DetailTab; sessionId: number | null }>();

const task = computed(() => state.tasks.find((t) => t.id === props.taskId) ?? null);
const sessions = computed(() => taskSessions(state, props.taskId));
const agents = computed(() => state.agents.filter((a) => a.available));
const terminal = computed<Session | null>(
  () =>
    sessions.value.find((s) => s.id === props.sessionId) ??
    sessions.value.find((s) => s.status !== 'exited') ??
    null,
);

const TABS = [
  { value: 'sessions', label: 'Sessions' },
  { value: 'terminal', label: 'Terminal' },
];

function show(tab: DetailTab, sessionId: number | null = null) {
  go({ name: 'task', taskId: props.taskId, tab, sessionId });
}

function selectTab(tab: string) {
  if (tab === 'sessions') show('sessions');
  else show('terminal', terminal.value?.id ?? null);
}

// Not sessionActions.startSession: on mobile a new session should open its terminal right away.
function start(kind: SessionKind) {
  api
    .startSession(props.taskId, kind)
    .then((s) => {
      addSession(state, s);
      show('terminal', s.id);
    })
    .catch((e) => toast(errorMessage(e)));
}

function archive() {
  if (!task.value || !window.confirm(`Archive "${task.value.title}"?`)) return;
  archiveTask(task.value);
  go({ name: 'list' });
}
</script>

<template>
  <div v-if="task" class="task-detail">
    <header>
      <Button
        variant="ghost"
        class="back"
        :icon="ChevronLeft"
        aria-label="Back"
        @click="go({ name: 'list' })"
      />
      <h1>{{ task.title }}</h1>
      <Button variant="ghost" :icon="Archive" aria-label="Archive task" @click="archive" />
    </header>
    <Tabs :model-value="tab" :tabs="TABS" class="tabs" @update:model-value="selectTab">
      <template #sessions>
        <section class="sessions">
          <div v-for="s in sessions" :key="s.id" class="session">
            <button class="open" @click="show('terminal', s.id)">
              <span class="status"><StatusIndicator :status="s.status" show-all /></span>
              {{ sessionLabel(s) }}
            </button>
            <Button @click="closeSession(s)">Close</Button>
          </div>
          <p v-if="!sessions.length" class="empty">No sessions</p>
          <div class="start">
            <Button
              v-for="a in agents"
              :key="a.name"
              @click="start({ type: 'agent', name: a.name })"
            >
              + {{ a.display_name || a.name }}
            </Button>
            <Button @click="start({ type: 'shell' })">+ Terminal</Button>
          </div>
        </section>
      </template>
      <template #terminal>
        <section class="terminal">
          <MobileTerminal v-if="terminal" :session="terminal" />
          <p v-else class="empty">No running session</p>
        </section>
      </template>
    </Tabs>
  </div>
  <p v-else class="empty">This task no longer exists.</p>
</template>

<style scoped>
.task-detail {
  display: flex;
  flex-direction: column;
  height: 100%;
}
header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px;
}
h1 {
  flex: 1;
  font-size: 16px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tabs {
  flex: 1;
}
.sessions,
.terminal {
  height: 100%;
  overflow: auto;
}
.session {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 44px;
  padding: 0 8px;
}
.open {
  flex: 1;
  min-height: 44px;
  justify-content: flex-start;
  gap: 8px;
  border: none;
  background: none;
}
.status {
  display: inline-flex;
  justify-content: center;
  flex: none;
  width: 14px;
}
.start {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 8px;
}
@media (min-width: 768px) {
  .back {
    display: none;
  }
}
</style>
