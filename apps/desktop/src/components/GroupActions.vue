<script setup lang="ts">
import type { DockviewApi, IDockviewGroupPanel } from 'dockview-vue';
import { Plus } from 'lucide-vue-next';
import { addTool, paneState } from '../dock/main';
import type { ToolPane } from '../dock/model';
import { newSessionMenu } from '../sessionActions';
import { state } from '../store';

const props = defineProps<{ params: { group: IDockviewGroupPanel; containerApi: DockviewApi } }>();
const TOOLS: { pane: ToolPane; label: string }[] = [{ pane: 'diff', label: 'Diff' }, { pane: 'activity', label: 'Activity Monitor' }];

function open(e: MouseEvent) {
  if (state.selectedTaskId === null) return;
  const { group, containerApi } = props.params;
  const closedTools = TOOLS.filter((t) => paneState(t.pane) === 'closed')
    .map((t) => ({ label: t.label, action: () => addTool(containerApi, t.pane, { group: group.id }) }));
  newSessionMenu(e, state.selectedTaskId, group.id, closedTools, group.api.location.type === 'floating');
}
</script>

<template>
  <button class="new-session" title="New tab" aria-label="New tab" @pointerdown.stop @click.stop="open"><Plus /></button>
</template>

<style scoped>
.new-session { border: 0; background: transparent; padding: 0 8px; height: 100%; display: flex; align-items: center; color: var(--muted); }
.new-session:hover { color: var(--text); }
</style>
