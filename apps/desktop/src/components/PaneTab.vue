<script setup lang="ts">
import type { DockviewApi, DockviewPanelApi } from 'dockview-vue';
import { X } from '@lucide/vue';
import { computed } from 'vue';
import { dockFloating, floatUnlocked } from '../dock/main';
import { closeSession } from '../sessionActions';
import { sessionLabel, showMenu, state, type MenuItem } from '../store';
import StatusIndicator from './StatusIndicator.vue';

const props = defineProps<{
  params: { params: { sessionId?: number }; api: DockviewPanelApi; containerApi: DockviewApi };
}>();
const session = computed(
  () => state.sessions.find((s) => s.id === props.params.params.sessionId) ?? null,
);
const label = computed(() =>
  session.value ? sessionLabel(session.value) : (props.params.api.title ?? ''),
);

function close() {
  if (session.value) closeSession(session.value);
  else props.params.api.close();
}

function split(position: 'right' | 'bottom') {
  props.params.api.moveTo({ group: props.params.api.group, position });
}

function menu(e: MouseEvent) {
  const { api, containerApi } = props.params;
  const floating = api.group.api.location.type === 'floating';
  // Moving a group's only tab beside its own group is not a split; dockview's drag and drop refuses it too.
  const canSplit = !floating && api.group.panels.length > 1;
  const items: MenuItem[] = [];
  if (canSplit)
    items.push(
      { label: 'Split right', action: () => split('right') },
      { label: 'Split down', action: () => split('bottom') },
    );
  if (floatUnlocked.value && !floating) {
    items.push({
      label: 'Float',
      action: () => {
        const panel = containerApi.getPanel(api.id);
        if (panel) containerApi.addFloatingGroup(panel);
      },
    });
  }
  if (floating) items.push({ label: 'Dock', action: () => dockFloating(containerApi) });
  items.push({
    label: session.value ? 'Close session' : 'Close',
    danger: !!session.value,
    action: close,
  });
  showMenu(e, items);
}
</script>

<template>
  <div
    class="pane-tab"
    :class="{ exited: session?.status === 'exited' }"
    @contextmenu.prevent.stop="menu"
  >
    <StatusIndicator v-if="session" :status="session.status" show-all />
    <span>{{ label }}</span>
    <button
      class="close"
      :aria-label="`Close ${label}`"
      :title="`Close ${label}`"
      @pointerdown.stop
      @click.stop="close"
    >
      <X />
    </button>
  </div>
</template>

<style scoped>
.pane-tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 4px 0 10px;
  height: 100%;
}
.pane-tab.exited {
  color: var(--muted);
}
.close {
  border: 0;
  background: transparent;
  padding: 0 3px;
  border-radius: 4px;
  opacity: 0.5;
}
.close:hover {
  opacity: 1;
  background: var(--border);
}
</style>
