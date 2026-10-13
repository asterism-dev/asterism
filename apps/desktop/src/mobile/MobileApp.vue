<script setup lang="ts">
import NewTaskDialog from '../components/NewTaskDialog.vue';
import { state } from '../store';
import { paired, revoked } from './connection';
import PairScreen from './PairScreen.vue';
import TaskDetail from './TaskDetail.vue';
import TaskList from './TaskList.vue';
import { route } from './useRoute';

const reload = () => window.location.reload();
</script>

<template>
  <PairScreen v-if="!paired" :revoked="revoked" />
  <div v-else class="mobile">
    <div v-if="state.node.state === 'disconnected'" class="banner">Disconnected, reconnecting…</div>
    <div v-else-if="state.node.state === 'connecting'" class="banner">Connecting…</div>
    <div v-else-if="state.node.state === 'incompatible'" class="banner error">
      {{ state.node.message }} <button @click="reload">Reload</button>
    </div>
    <main class="body" :class="{ 'has-task': route.name === 'task' }">
      <TaskList class="list-pane" />
      <section class="detail-pane">
        <TaskDetail
          v-if="route.name === 'task'"
          :key="route.taskId"
          :task-id="route.taskId"
          :tab="route.tab"
          :session-id="route.sessionId"
        />
        <p v-else class="empty">Select a task</p>
      </section>
    </main>
    <NewTaskDialog
      v-if="state.newTaskFor !== null"
      :project-id="state.newTaskFor"
      @close="state.newTaskFor = null"
    />
    <div class="toasts">
      <div v-for="t in state.toasts" :key="t.id" class="toast">{{ t.message }}</div>
    </div>
  </div>
</template>

<style scoped>
.mobile {
  display: flex;
  flex-direction: column;
  height: 100dvh;
  padding: env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom)
    env(safe-area-inset-left);
}
.body {
  flex: 1;
  min-height: 0;
  display: grid;
  /* minmax(0, …): wide content such as the key bar must not widen the page */
  grid-template-columns: minmax(0, 1fr);
}
@media (max-width: 767.98px) {
  .body.has-task .list-pane,
  .body:not(.has-task) .detail-pane {
    display: none;
  }
}
@media (min-width: 768px) {
  .body {
    grid-template-columns: 320px minmax(0, 1fr);
  }
}
</style>
