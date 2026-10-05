<script setup lang="ts">
import type { DockviewPanelApi } from 'dockview-vue';
import { computed } from 'vue';
import { closeSession } from '../sessionActions';
import { sessionLabel, showMenu, state } from '../store';
import StatusIndicator from './StatusIndicator.vue';

const props = defineProps<{ params: { params: { sessionId: number }; api: DockviewPanelApi } }>();
const session = computed(() => state.sessions.find((s) => s.id === props.params.params.sessionId) ?? null);

function split(position: 'right' | 'bottom') {
  props.params.api.moveTo({ group: props.params.api.group, position });
}

function menu(e: MouseEvent) {
  const s = session.value;
  if (!s) return;
  // Moving a group's only tab beside its own group is not a split; dockview's drag and drop refuses it too.
  const canSplit = props.params.api.group.panels.length > 1;
  showMenu(e, [
    ...(canSplit ? [
      { label: 'Split right', action: () => split('right') },
      { label: 'Split down', action: () => split('bottom') },
    ] : []),
    { label: 'Close session', danger: true, action: () => closeSession(s) },
  ]);
}
</script>

<template>
  <div v-if="session" class="session-tab" :class="{ exited: session.status === 'exited' }" @contextmenu.prevent.stop="menu">
    <StatusIndicator :status="session.status" show-all />
    <span>{{ sessionLabel(session) }}</span>
    <button class="close" aria-label="Close session" title="Close session" @pointerdown.stop @click.stop="closeSession(session)">×</button>
  </div>
</template>

<style scoped>
.session-tab { display: flex; align-items: center; gap: 6px; padding: 0 4px 0 10px; height: 100%; }
.session-tab.exited { color: var(--muted); }
.close { border: 0; background: transparent; padding: 0 3px; border-radius: 4px; opacity: 0.5; }
.close:hover { opacity: 1; background: var(--border); }
</style>
