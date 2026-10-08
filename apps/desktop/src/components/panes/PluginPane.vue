<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { api } from '../../api';
import { frameLoaded, pluginUrl, registerFrame } from '../../pluginFrames';
import type { PanelContext } from '../../pluginBridge';
import { state } from '../../store';
import type { PluginInfo } from '../../types';

const props = defineProps<{ params: { params: { plugin: string; panel: string } } }>();
const frame = ref<HTMLIFrameElement | null>(null);
const src = ref<string | null>(null);
const notice = ref('Loading…');
let plugin: PluginInfo | undefined;
let ctx: PanelContext | null = null;
let unregister: (() => void) | null = null;

async function load() {
  const { plugin: name, panel: id } = props.params.params;
  plugin = (await api.plugins()).find((p) => p.name === name);
  const panel = (plugin?.panels ?? []).find((p) => p.id === id);
  if (!plugin || !panel) notice.value = `The plugin ${name} is not installed.`;
  else if (plugin.state.state !== 'ok')
    notice.value = `The plugin ${name} is not available (${plugin.state.state}).`;
  else {
    if (ctx) ctx.permissions = plugin.permissions;
    return void (src.value = pluginUrl(name, panel.entry));
  }
  src.value = null;
}

// Register as soon as the iframe exists: its module script may call the bridge before `load`.
function register(el: HTMLIFrameElement | null) {
  unregister?.();
  unregister = null;
  ctx = null;
  if (!el || !plugin || state.selectedTaskId === null) return;
  ctx = { taskId: state.selectedTaskId, permissions: plugin.permissions, subscribed: false };
  unregister = registerFrame(el, ctx);
}

// Dockview re-attaching the pane reloads the iframe in a new window, which needs the theme again.
function onLoad() {
  if (frame.value) frameLoaded(frame.value);
}

onMounted(load);
watch(() => state.pluginsVersion, load);
watch(frame, register, { flush: 'post' });
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
