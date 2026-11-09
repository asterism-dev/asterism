<script setup lang="ts">
import { Plus } from '@lucide/vue';
import { computed, reactive, ref, watch } from 'vue';
import StatusIndicator from '../components/StatusIndicator.vue';
import { groupProjects, loadSortMode } from '../projects';
import { state, switchWorkspace, taskStatus, waitingSessions } from '../store';
import { loadMoreOpen, saveMoreOpen, waitingByWorkspace } from '../workspaces';
import { go, route } from './useRoute';

const query = ref('');
const sortMode = loadSortMode();
const waitingTasks = computed(() => new Set(waitingSessions(state).map((s) => s.task_id)));
const grouped = computed(() =>
  groupProjects(state.projects, state.tasks, state.activeWorkspace, query.value, sortMode),
);
const waitingCounts = computed(() =>
  waitingByWorkspace(state.projects, state.tasks, waitingSessions(state)),
);
const openByWorkspace = reactive<Record<number, boolean>>(loadMoreOpen());
const moreOpen = computed(
  () => state.activeWorkspace !== null && !!openByWorkspace[state.activeWorkspace],
);
function toggleMore() {
  if (state.activeWorkspace !== null) openByWorkspace[state.activeWorkspace] = !moreOpen.value;
}
watch(openByWorkspace, saveMoreOpen, { deep: true });
const selected = computed(() => (route.value.name === 'task' ? route.value.taskId : null));
</script>

<template>
  <nav class="task-list">
    <div v-if="state.workspaces.length > 1" class="workspaces">
      <button
        v-for="w in state.workspaces"
        :key="w.id"
        type="button"
        class="chip"
        :class="{ active: state.activeWorkspace === w.id }"
        :aria-pressed="state.activeWorkspace === w.id"
        @click="switchWorkspace(state, w.id)"
      >
        {{ w.name }}
        <span v-if="state.activeWorkspace !== w.id && waitingCounts[w.id]" class="badge">
          {{ waitingCounts[w.id] }}<span class="sr-only"> waiting</span>
        </span>
      </button>
    </div>
    <input v-model="query" type="search" placeholder="Search projects and tasks" />
    <section v-for="g in grouped.active" :key="g.project.id">
      <header>
        <h2>{{ g.project.name }}</h2>
        <button class="icon" aria-label="New task" @click="state.newTaskFor = g.project.id">
          <Plus :size="18" />
        </button>
      </header>
      <button
        v-for="t in g.tasks"
        :key="t.id"
        class="task"
        :class="{ selected: t.id === selected, waiting: waitingTasks.has(t.id) }"
        @click="go({ name: 'task', taskId: t.id, tab: 'sessions', sessionId: null })"
      >
        <span class="status"><StatusIndicator :status="taskStatus(state, t.id)" /></span>
        <span class="title">{{ t.title }}</span>
      </button>
      <p v-if="!g.tasks.length" class="empty">No tasks</p>
    </section>
    <button
      v-if="grouped.idle.length"
      type="button"
      class="more"
      :aria-expanded="moreOpen"
      @click="toggleMore"
    >
      <span aria-hidden="true">{{ moreOpen ? '⌄' : '›' }}</span> {{ grouped.idle.length }} more
      projects
    </button>
    <template v-if="moreOpen">
      <section v-for="g in grouped.idle" :key="g.project.id">
        <header>
          <h2>{{ g.project.name }}</h2>
          <button class="icon" aria-label="New task" @click="state.newTaskFor = g.project.id">
            <Plus :size="18" />
          </button>
        </header>
      </section>
    </template>
    <p v-if="!grouped.active.length && !grouped.idle.length" class="empty">Nothing found</p>
  </nav>
</template>

<style scoped>
.task-list {
  overflow-y: auto;
  height: 100%;
  padding: 8px;
}
.workspaces {
  display: flex;
  gap: 8px;
  overflow-x: auto;
  margin-bottom: 8px;
}
.chip {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 44px;
  padding: 0 14px;
  border-radius: 22px;
  white-space: nowrap;
}
.chip.active {
  background: var(--select);
}
.badge {
  background: var(--waiting);
  color: #1d1f27;
  padding: 0 6px;
  border-radius: 10px;
  font-size: 12px;
}
.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
}
input {
  width: 100%;
  font-size: 16px; /* below 16px iOS zooms into the field */
  margin-bottom: 8px;
}
header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
button.icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 44px;
  min-height: 44px;
}
h2 {
  font-size: 13px;
  text-transform: uppercase;
  opacity: 0.7;
}
.more {
  width: 100%;
  min-height: 44px;
  text-align: left;
  border: none;
  background: none;
  opacity: 0.7;
}
.task {
  display: flex;
  justify-content: flex-start;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 44px;
  text-align: left;
  border: none;
  background: none;
}
.status {
  display: inline-flex;
  justify-content: center;
  flex: none;
  width: 14px;
}
.task.selected {
  background: var(--select);
}
.task.waiting .title {
  font-weight: 600;
}
.title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
