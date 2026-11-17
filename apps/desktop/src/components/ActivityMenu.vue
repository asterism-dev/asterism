<script setup lang="ts">
import { relativeTime } from '../projects';
import { state } from '../store';
import ActivitySteps from './ActivitySteps.vue';

const now = Date.now() / 1000;
</script>

<template>
  <div class="activity-menu">
    <p v-if="!state.activities.length" class="empty muted">No recent activity</p>
    <section v-for="a in state.activities" :key="a.id" class="item">
      <header>
        <span class="title">{{ a.title }}</span>
        <span class="muted">{{ relativeTime(a.started_at, now) }}</span>
      </header>
      <ActivitySteps :steps="a.steps" />
    </section>
  </div>
</template>

<style scoped>
.activity-menu {
  width: 320px;
  max-height: 60vh;
  overflow-y: auto;
}
.item {
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.item + .item {
  border-top: 1px solid var(--border);
}
header {
  display: flex;
  gap: 8px;
  align-items: baseline;
}
.title {
  flex: 1;
  min-width: 0;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.empty {
  margin: 0;
  padding: 8px;
}
</style>
