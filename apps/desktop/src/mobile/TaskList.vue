<script setup lang="ts">
import { Plus } from '@lucide/vue';
import { computed, ref } from 'vue';
import StatusIndicator from '../components/StatusIndicator.vue';
import { filterTasks, loadSortMode, sortProjects, sortTasks } from '../projects';
import { state, taskStatus, waitingSessions } from '../store';
import { go, route } from './useRoute';

const query = ref('');
const sortMode = loadSortMode();
const waitingTasks = computed(() => new Set(waitingSessions(state).map((s) => s.task_id)));
const groups = computed(() =>
  sortProjects(state.projects, state.tasks, sortMode).flatMap((project) => {
    const tasks = filterTasks(project, state.tasks, query.value);
    return tasks === null ? [] : [{ project, tasks: sortTasks(tasks, sortMode) }];
  }),
);
const selected = computed(() => (route.value.name === 'task' ? route.value.taskId : null));
</script>

<template>
  <nav class="task-list">
    <input v-model="query" type="search" placeholder="Search projects and tasks" />
    <section v-for="g in groups" :key="g.project.id">
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
        <StatusIndicator :status="taskStatus(state, t.id)" />
        <span class="title">{{ t.title }}</span>
      </button>
      <p v-if="!g.tasks.length" class="empty">No tasks</p>
    </section>
    <p v-if="!groups.length" class="empty">Nothing found</p>
  </nav>
</template>

<style scoped>
.task-list {
  overflow-y: auto;
  height: 100%;
  padding: 8px;
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
.task {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 44px;
  text-align: left;
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
