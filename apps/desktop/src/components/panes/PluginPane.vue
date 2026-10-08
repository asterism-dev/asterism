<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { api } from '../../api';
import { pluginUrl, registerFrame } from '../../pluginFrames';
import { state } from '../../store';
import type { PluginInfo } from '../../types';

const props = defineProps<{ params: { params: { plugin: string; panel: string } } }>();
const frame = ref<HTMLIFrameElement | null>(null);
const src = ref<string | null>(null);
const notice = ref('Loading…');
let plugin: PluginInfo | undefined;
let unregister: (() => void) | null = null;

async function load() {
  const { plugin: name, panel: id } = props.params.params;
  plugin = (await api.plugins()).find((p) => p.name === name);
  const panel = plugin?.panels.find((p) => p.id === id);
  if (!plugin || !panel) notice.value = `The plugin ${name} is not installed.`;
  else if (plugin.state.state !== 'ok')
    notice.value = `The plugin ${name} is not available (${plugin.state.state}).`;
  else return void (src.value = pluginUrl(name, panel.entry));
  src.value = null;
}

function onLoad() {
  unregister?.();
  const win = frame.value?.contentWindow;
  if (!win || !plugin || state.selectedTaskId === null) return;
  unregister = registerFrame(win, {
    taskId: state.selectedTaskId,
    permissions: plugin.permissions,
    subscribed: false,
  });
}

onMounted(load);
watch(() => state.pluginsVersion, load);
onBeforeUnmount(() => unregister?.());
</script>

<template>
  <iframe
    v-if="src"
    ref="frame"
    class="plugin-frame"
    :src="src"
    sandbox="allow-scripts"
    @load="onLoad"
  />
  <div v-else class="plugin-notice">{{ notice }}</div>
</template>

<style scoped>
.plugin-frame {
  border: 0;
  width: 100%;
  height: 100%;
  display: block;
  background: var(--bg);
}
.plugin-notice {
  padding: 16px;
  color: var(--muted);
}
</style>
