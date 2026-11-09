<script setup lang="ts">
import { ArrowLeft } from '@lucide/vue';
import { computed } from 'vue';
import { api, errorMessage } from '../api';
import { relativeTime } from '../projects';
import { leaveSettings } from '../settingsGuard';
import {
  historyNotifications,
  notificationTarget,
  selectSession,
  sessionLabel,
  state,
  taskSessions,
  toast,
  waitingNotifications,
} from '../store';
import type { NotificationEntry } from '../types';

defineProps<{ now: number }>();

const report = (e: unknown) => toast(errorMessage(e));
const waiting = computed(() => waitingNotifications(state));
const history = computed(() => historyNotifications(state));
const hasUnread = computed(() => history.value.some((n) => n.read_at === null));
const groups = computed(() =>
  [
    { title: 'Waiting for you', entries: waiting.value },
    { title: 'History', entries: history.value },
  ].filter((g) => g.entries.length),
);

function detail(n: NotificationEntry): string {
  const session = state.sessions.find((s) => s.id === n.session_id);
  const multiple = n.task_id !== null && taskSessions(state, n.task_id).length > 1;
  return [n.project_name, multiple && session ? sessionLabel(session) : null]
    .filter(Boolean)
    .join(' · ');
}

function open(n: NotificationEntry) {
  const target = notificationTarget(state, n);
  if (!target) return;
  // Waiting entries are never read; resolving them already set read_at.
  if (n.read_at === null && n.kind !== 'waiting') api.readNotification(n.id).catch(report);
  leaveSettings()
    .then((left) => {
      if (!left) return;
      if ('session' in target) selectSession(state, target.session);
      else {
        state.projectPage = null;
        state.selectedTaskId = target.taskId;
      }
    })
    .catch(report);
}
</script>

<template>
  <div class="notifications">
    <div class="row header">
      <button class="back" aria-label="Back to projects" @click="state.notificationsOpen = false">
        <ArrowLeft />
      </button>
      <span class="name">Notifications</span>
      <button v-if="hasUnread" class="link" @click="api.readAllNotifications().catch(report)">
        Mark all read
      </button>
    </div>
    <p v-if="!groups.length" class="hint muted">Nothing here.</p>
    <section v-for="g in groups" :key="g.title">
      <div class="group muted">{{ g.title }}</div>
      <div
        v-for="n in g.entries"
        :key="n.id"
        class="entry"
        :class="{
          resolved: n.resolved_at !== null,
          unread: n.read_at === null && n.kind !== 'waiting',
          inert: !notificationTarget(state, n),
        }"
        @click="open(n)"
      >
        <span class="dot" :class="{ waiting: n.kind === 'waiting' }" />
        <div class="text">
          <div class="title">{{ n.task_title ?? 'Unknown task' }}</div>
          <div class="muted">{{ detail(n) }}</div>
        </div>
        <span class="age muted">{{ relativeTime(n.created_at, now) }}</span>
      </div>
    </section>
  </div>
</template>

<style scoped>
.header .name {
  font-weight: 600;
}
.back,
.link {
  background: none;
  border: 0;
  padding: 0 4px;
}
.link {
  font-size: 12px;
}
.group {
  font-size: 11px;
  text-transform: uppercase;
  padding: 10px 6px 4px;
}
.entry {
  display: flex;
  gap: 8px;
  padding: 6px;
  border-radius: 6px;
  cursor: pointer;
}
.entry:hover {
  background: var(--select);
}
.entry.inert {
  cursor: default;
}
.entry.inert:hover {
  background: none;
}
.entry.resolved {
  opacity: 0.4;
}
.entry.unread .title {
  font-weight: 600;
}
.dot {
  flex: none;
  width: 8px;
  height: 8px;
  margin-top: 5px;
  border-radius: 50%;
  background: var(--accent);
}
.dot.waiting {
  background: var(--waiting);
}
.text {
  flex: 1;
  min-width: 0;
}
.text > div {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.text .muted {
  font-size: 12px;
}
.age {
  flex: none;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}
.hint {
  padding: 6px;
}
</style>
