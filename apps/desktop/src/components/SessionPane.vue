<script setup lang="ts">
import type { DockviewPanelApi } from 'dockview-vue';
import { computed, onUnmounted, ref } from 'vue';
import { state } from '../store';
import TerminalPane from './TerminalPane.vue';

const props = defineProps<{ params: { params: { sessionId: number }; api: DockviewPanelApi } }>();
const session = computed(
  () => state.sessions.find((s) => s.id === props.params.params.sessionId) ?? null,
);
// dockview keeps hidden panels mounted, so the terminal is torn down here to detach it like an inactive tab.
const visible = ref(props.params.api.isVisible);
const subscription = props.params.api.onDidVisibilityChange((e) => {
  visible.value = e.isVisible;
});
onUnmounted(() => subscription.dispose());
</script>

<template>
  <div class="pane">
    <TerminalPane
      v-if="session && visible"
      :key="`${session.id}-${session.status === 'exited'}`"
      :session-id="session.id"
      :task-id="session.task_id"
      :live="session.status !== 'exited'"
    />
  </div>
</template>
