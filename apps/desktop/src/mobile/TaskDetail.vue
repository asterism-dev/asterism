<script setup lang="ts">
import { Archive, ChevronLeft } from '@lucide/vue';
import { computed } from 'vue';
import StatusIndicator from '../components/StatusIndicator.vue';
import ReviewView from '../components/review/ReviewView.vue';
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

function show(tab: DetailTab, sessionId: number | null = null) {
  go({ name: 'task', taskId: props.taskId, tab, sessionId });
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
      <button class="icon back" aria-label="Back" @click="go({ name: 'list' })">
        <ChevronLeft :size="20" />
      </button>
      <h1>{{ task.title }}</h1>
      <button class="icon" aria-label="Archive task" @click="archive">
        <Archive :size="18" />
      </button>
    </header>
    <nav class="tabs">
      <button :class="{ active: tab === 'sessions' }" @click="show('sessions')">Sessions</button>
      <button
        :class="{ active: tab === 'terminal' }"
        @click="show('terminal', terminal?.id ?? null)"
      >
        Terminal
      </button>
      <button :class="{ active: tab === 'review' }" @click="show('review')">Review</button>
    </nav>
    <section v-if="tab === 'sessions'" class="sessions">
      <div v-for="s in sessions" :key="s.id" class="session">
        <button class="open" @click="show('terminal', s.id)">
          <StatusIndicator :status="s.status" />
          {{ sessionLabel(s) }}
        </button>
        <button @click="closeSession(s)">Close</button>
      </div>
      <p v-if="!sessions.length" class="empty">No sessions</p>
      <div class="start">
        <button v-for="a in agents" :key="a.name" @click="start({ type: 'agent', name: a.name })">
          + {{ a.display_name || a.name }}
        </button>
        <button @click="start({ type: 'shell' })">+ Terminal</button>
      </div>
    </section>
    <section v-else-if="tab === 'terminal'" class="terminal">
      <MobileTerminal v-if="terminal" :session="terminal" />
      <p v-else class="empty">No running session</p>
    </section>
    <section v-else class="review">
      <ReviewView :task="task" />
    </section>
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
.icon {
  min-width: 44px;
  min-height: 44px;
}
.tabs {
  display: flex;
}
.tabs button {
  flex: 1;
  min-height: 44px;
}
.tabs button.active {
  font-weight: 600;
  border-bottom: 2px solid currentColor;
}
.sessions,
.terminal,
.review {
  flex: 1;
  min-height: 0;
  overflow: auto;
}
.session {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 44px;
  padding: 0 8px;
}
.session button,
.start button {
  min-height: 44px;
}
.open {
  display: flex;
  align-items: center;
  gap: 8px;
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
