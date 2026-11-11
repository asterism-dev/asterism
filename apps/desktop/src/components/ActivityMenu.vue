<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue';
import { relativeTime } from '../projects';
import { state } from '../store';
import ActivitySteps from './ActivitySteps.vue';

const emit = defineEmits<{ close: [] }>();
const now = Date.now() / 1000;

const close = () => emit('close');
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') close();
}

onMounted(() => {
  window.addEventListener('click', close);
  window.addEventListener('keydown', onKey);
});
onUnmounted(() => {
  window.removeEventListener('click', close);
  window.removeEventListener('keydown', onKey);
});
</script>

<template>
  <div class="activity-menu" role="dialog" aria-label="Activity" @click.stop>
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
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  z-index: 10002;
  width: 320px;
  max-height: 60vh;
  overflow-y: auto;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 4px;
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.15);
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
