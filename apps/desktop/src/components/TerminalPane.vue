<script setup lang="ts">
import { Channel } from '@tauri-apps/api/core';
import { FitAddon } from '@xterm/addon-fit';
import { WebglAddon } from '@xterm/addon-webgl';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { api, decodeBase64, errorMessage } from '../api';
import { isConnected, state, toast } from '../store';

const props = defineProps<{ sessionId: number; live: boolean }>();

const RESIZE_DEBOUNCE_MS = 50;
const FINAL_SCREEN_LINES = 2000;

const el = ref<HTMLDivElement>();
let term: Terminal | null = null;
let fit: FitAddon | null = null;
let observer: ResizeObserver | null = null;
let resizeTimer: number | undefined;
let attached = false;
let disposed = false;
let generation = 0;

function syncSize() {
  if (!term || !fit || !el.value) return;
  if (el.value.clientWidth === 0 || el.value.clientHeight === 0) return;
  fit.fit();
  // A hidden or collapsed pane measures 0×0; the daemon must never be resized to that.
  if (!props.live || !attached || term.rows === 0 || term.cols === 0) return;
  const { rows, cols } = term;
  clearTimeout(resizeTimer);
  resizeTimer = window.setTimeout(() => api.resize(props.sessionId, rows, cols).catch(() => {}), RESIZE_DEBOUNCE_MS);
}

async function attach() {
  if (!term) return;
  const mine = ++generation;
  term.reset();
  attached = false;
  // Channel messages may overtake the invoke result, so frames wait until the snapshot is painted.
  const pending: string[] = [];
  let painted = false;
  const channel = new Channel<string>();
  channel.onmessage = (data) => {
    if (painted) term?.write(decodeBase64(data));
    else pending.push(data);
  };
  try {
    const result = await api.attach(props.sessionId, channel);
    if (disposed || mine !== generation) {
      // A newer attach owns the session's output sink; only detach when nobody else does.
      if (mine === generation) api.detach(props.sessionId).catch(() => {});
      return;
    }
    term.write(decodeBase64(result.snapshot));
    painted = true;
    pending.forEach((data) => term?.write(decodeBase64(data)));
    attached = true;
    syncSize();
  } catch (e) {
    if (!disposed && mine === generation) toast(errorMessage(e));
  }
}

async function showFinalScreen() {
  try {
    const read = await api.read(props.sessionId, FINAL_SCREEN_LINES);
    term?.write(read.text.replace(/\n/g, '\r\n'));
  } catch (e) {
    toast(errorMessage(e));
  }
}

onMounted(() => {
  if (!el.value) return;
  term = new Terminal({
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
    fontSize: 13,
    scrollback: 2000,
    cursorBlink: props.live,
    disableStdin: !props.live,
  });
  fit = new FitAddon();
  term.loadAddon(fit);
  term.open(el.value);
  try {
    term.loadAddon(new WebglAddon());
  } catch {
    // WebGL can be unavailable (e.g. some Linux GPUs); xterm keeps its DOM renderer.
  }
  observer = new ResizeObserver(syncSize);
  observer.observe(el.value);
  fit.fit();
  if (props.live) {
    term.onData((data) => api.send(props.sessionId, data).catch(() => {}));
    if (isConnected(state.node)) attach();
  } else {
    showFinalScreen();
  }
  term.focus();
});

watch(
  () => isConnected(state.node),
  (connected) => {
    if (connected && props.live) attach();
    else attached = false;
  },
);

onBeforeUnmount(() => {
  disposed = true;
  observer?.disconnect();
  clearTimeout(resizeTimer);
  if (props.live && attached) api.detach(props.sessionId).catch(() => {});
  term?.dispose();
  term = null;
  fit = null;
});
</script>

<template>
  <div ref="el" class="terminal"></div>
</template>

<style scoped>
.terminal { position: absolute; inset: 0; padding: 6px; background: #111217; }
</style>
