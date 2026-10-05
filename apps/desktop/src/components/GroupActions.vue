<script setup lang="ts">
import type { IDockviewGroupPanel } from 'dockview-vue';
import { newSessionMenu } from '../sessionActions';
import { state } from '../store';

const props = defineProps<{ params: { group: IDockviewGroupPanel } }>();

function open(e: MouseEvent) {
  if (state.selectedTaskId !== null) newSessionMenu(e, state.selectedTaskId, props.params.group.id);
}
</script>

<template>
  <button class="new-session" title="New session" aria-label="New session" @pointerdown.stop @click.stop="open">+</button>
</template>

<style scoped>
.new-session { border: 0; background: transparent; padding: 2px 8px; color: var(--muted); }
.new-session:hover { color: var(--text); }
</style>
