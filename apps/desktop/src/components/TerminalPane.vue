<script setup lang="ts">
import { Channel } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { WebglAddon } from '@xterm/addon-webgl';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { api, decodeBase64, errorMessage, RpcError } from '../api';
import { openFile } from '../dock/main';
import { findFileLinks } from '../fileLinks';
import { matchAction } from '../shortcuts';
import { isConnected, refresh, state, toast } from '../store';

const props = defineProps<{ sessionId: number; taskId: number; live: boolean }>();

const RESIZE_DEBOUNCE_MS = 50;
const FINAL_SCREEN_LINES = 2000;

const el = ref<HTMLDivElement>();
let term: Terminal | null = null;
let fit: FitAddon | null = null;
let observer: ResizeObserver | null = null;
let resizeTimer: number | undefined;
let attached = false;
let replaying = false;
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
  resizeTimer = window.setTimeout(
    () => api.resize(props.sessionId, rows, cols).catch(() => {}),
    RESIZE_DEBOUNCE_MS,
  );
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
    if (disposed || mine !== generation) return;
    term.resize(result.cols, result.rows);
    // Replayed output repeats old terminal queries; xterm's answers to them must not reach the process.
    replaying = true;
    term.write(decodeBase64(result.snapshot), () => {
      if (mine === generation) replaying = false;
    });
    painted = true;
    pending.forEach((data) => term?.write(decodeBase64(data)));
    attached = true;
    syncSize();
  } catch (e) {
    if (disposed || mine !== generation) return;
    // The session ended while detached; the refreshed status swaps this pane for its exited view.
    if (e instanceof RpcError && e.kind === 'not_found')
      refresh().catch((err) => toast(errorMessage(err)));
    else toast(errorMessage(e));
  }
}

async function showFinalScreen() {
  try {
    const read = await api.read(props.sessionId, FINAL_SCREEN_LINES);
    term?.write(read.text.replace(/\n/g, '\r\n'));
  } catch (e) {
    // A just-removed session has no final screen; its tab is about to disappear.
    if (!(e instanceof RpcError && e.kind === 'not_found')) toast(errorMessage(e));
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
  // Modifier-click keeps plain clicks free for focus, selection and TUI mouse input.
  term.loadAddon(
    new WebLinksAddon((e, uri) => {
      if (e.metaKey || e.ctrlKey) openUrl(uri).catch((err) => toast(errorMessage(err)));
    }),
  );
  // ponytail: string index = cell column, so wide characters before a path shift its underline; map via the buffer's cells if that shows up.
  term.registerLinkProvider({
    provideLinks(y, callback) {
      const text = term?.buffer.active.getLine(y - 1)?.translateToString(true) ?? '';
      callback(
        findFileLinks(text).map((link) => ({
          range: { start: { x: link.start + 1, y }, end: { x: link.end, y } },
          text: link.path,
          activate: (e: MouseEvent) => {
            if (e.metaKey || e.ctrlKey) void openFile(props.taskId, link.path, link.line);
          },
        })),
      );
    },
  });
  // Linux app shortcuts are Ctrl+Shift chords xterm would otherwise consume.
  term.attachCustomKeyEventHandler((e) => matchAction(e, { inTerminal: true }) === null);
  try {
    const webgl = new WebglAddon();
    webgl.onContextLoss(() => webgl.dispose());
    term.loadAddon(webgl);
  } catch {
    // WebGL can be unavailable (e.g. some Linux GPUs); xterm keeps its DOM renderer.
  }
  observer = new ResizeObserver(syncSize);
  observer.observe(el.value);
  fit.fit();
  if (props.live) {
    term.onData((data) => {
      if (!replaying) void api.send(props.sessionId, data);
    });
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
  // Unconditional: it must enter the session's queue before any later instance's attach.
  if (props.live) api.detach(props.sessionId).catch(() => {});
  term?.dispose();
  term = null;
  fit = null;
});
</script>

<template>
  <div ref="el" class="terminal"></div>
</template>

<style scoped>
.terminal {
  position: absolute;
  inset: 0;
  padding: 6px;
  background: #111217;
}
</style>
