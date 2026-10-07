<script setup lang="ts">
import type { SessionStatus } from '../types';

// The sidebar shows only states that need attention; tabs pass showAll to mark every state.
defineProps<{ status: SessionStatus | null; showAll?: boolean }>();

const LABELS: Record<SessionStatus, string> = {
  waiting_input: 'Waiting for input',
  working: 'Working',
  idle: 'Idle',
  exited: 'Exited',
};
</script>

<template>
  <svg
    v-if="status === 'working'"
    class="stars"
    viewBox="0 0 24 24"
    role="img"
    :aria-label="LABELS.working"
  >
    <title>{{ LABELS.working }}</title>
    <g class="links">
      <line x1="12" y1="5" x2="5.5" y2="17" />
      <line x1="12" y1="5" x2="18.5" y2="17" />
      <line x1="5.5" y1="17" x2="18.5" y2="17" />
    </g>
    <circle class="star" cx="12" cy="5" r="3" />
    <circle class="star" cx="18.5" cy="17" r="2.5" />
    <circle class="star" cx="5.5" cy="17" r="2.5" />
  </svg>
  <span
    v-else-if="status === 'waiting_input' || (status && showAll)"
    class="dot"
    :class="status"
    :title="LABELS[status]"
    role="img"
    :aria-label="LABELS[status]"
  ></span>
</template>

<style scoped>
.stars {
  width: 14px;
  height: 14px;
  flex: none;
  color: var(--accent);
  overflow: visible;
}
.links {
  stroke: currentColor;
  stroke-width: 1.2;
  opacity: 0.35;
}
.star {
  fill: currentColor;
  transform-box: fill-box;
  transform-origin: center;
  animation: twinkle 1.2s ease-in-out infinite;
}
.star:nth-of-type(2) {
  animation-delay: 0.4s;
}
.star:nth-of-type(3) {
  animation-delay: 0.8s;
}
@keyframes twinkle {
  0%,
  100% {
    opacity: 0.35;
    transform: scale(0.7);
  }
  40% {
    opacity: 1;
    transform: scale(1.15);
  }
}
@media (prefers-reduced-motion: reduce) {
  .star {
    animation: none;
    opacity: 1;
  }
}
</style>
