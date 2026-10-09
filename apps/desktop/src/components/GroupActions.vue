<script setup lang="ts">
import type { DockviewApi, IDockviewGroupPanel } from 'dockview-vue';
import { Plus } from '@lucide/vue';
import { api } from '../api';
import { addPluginPanel, addTool, paneState } from '../dock/main';
import { pluginPanelId, type ToolPane } from '../dock/model';
import { newSessionMenu } from '../sessionActions';
import { state } from '../store';

const props = defineProps<{ params: { group: IDockviewGroupPanel; containerApi: DockviewApi } }>();
const TOOLS: { pane: ToolPane; label: string }[] = [
  { pane: 'diff', label: 'Review' },
  { pane: 'activity', label: 'Activity Monitor' },
];

async function open(e: MouseEvent) {
  const taskId = state.selectedTaskId;
  if (taskId === null) return;
  const { group, containerApi } = props.params;
  const closedTools = TOOLS.filter((t) => paneState(t.pane) === 'closed').map((t) => ({
    label: t.label,
    action: () => addTool(containerApi, t.pane, { group: group.id }),
  }));
  // A failing plugin list must not cost the user the new-session menu.
  const plugins = await api.plugins().catch(() => []);
  const panels = plugins
    .filter((p) => p.state.state === 'ok')
    .flatMap((p) =>
      (p.panels ?? []).filter((x) => x.slot === 'task').map((x) => ({ plugin: p.name, panel: x })),
    )
    .filter(({ plugin, panel }) => !containerApi.getPanel(pluginPanelId(plugin, panel.id)))
    .map(({ plugin, panel }) => ({
      label: panel.title,
      action: () => addPluginPanel(containerApi, plugin, panel, { group: group.id }),
    }));
  newSessionMenu(
    e,
    taskId,
    group.id,
    [...closedTools, ...panels],
    group.api.location.type === 'floating',
  );
}
</script>

<template>
  <button
    class="new-session"
    title="New tab"
    aria-label="New tab"
    @pointerdown.stop
    @click.stop="open"
  >
    <Plus />
  </button>
</template>

<style scoped>
.new-session {
  border: 0;
  background: transparent;
  padding: 0 8px;
  height: 100%;
  display: flex;
  align-items: center;
  color: var(--muted);
}
.new-session:hover {
  color: var(--text);
}
</style>
