<script setup lang="ts">
import type { DockviewPanelApi } from 'dockview-vue';
import { computed, nextTick, onUnmounted, ref, watch } from 'vue';
import { api, errorMessage } from '../../api';
import { renderCode } from '../../fileView';

const props = defineProps<{ params: { params: { taskId: number; path: string; line?: number }; api: DockviewPanelApi } }>();

const POLL_MS = 1500;
const LINE_HEIGHT = 18;

const file = computed(() => props.params.params);
const content = ref<string | null>(null);
const error = ref<string | null>(null);
const scroller = ref<HTMLDivElement>();
const html = computed(() => (content.value === null ? '' : renderCode(file.value.path, content.value)));
const lineCount = computed(() => (content.value === null ? 0 : content.value.replace(/\n$/, '').split('\n').length));
let mtime: number | null = null;
let loading = false;
let timer: number | undefined;
let savedScroll = 0;
let jumpPending = false;

function scrollToLine() {
  const line = file.value.line;
  if (!line || !scroller.value) return;
  // Detached by dockview while hidden; retry once shown.
  jumpPending = scroller.value.clientHeight === 0;
  if (jumpPending) return;
  scroller.value.scrollTop = (line - 1) * LINE_HEIGHT - scroller.value.clientHeight / 2;
}

async function load() {
  if (loading) return;
  loading = true;
  try {
    const result = await api.file(file.value.taskId, file.value.path, mtime);
    const first = content.value === null;
    mtime = result.mtime;
    if (result.content !== null) content.value = result.content;
    error.value = null;
    if (first) {
      await nextTick();
      scrollToLine();
    }
  } catch (e) {
    // Forgetting the content makes the next successful poll refetch it and jump back to the line.
    error.value = errorMessage(e);
    content.value = null;
    mtime = null;
  } finally {
    loading = false;
  }
}

// dockview detaches hidden tabs before announcing it, so the position is recorded while scrolling instead of on hide.
function rememberScroll() {
  if (scroller.value && scroller.value.clientHeight > 0) savedScroll = scroller.value.scrollTop;
}

function setPolling(visible: boolean) {
  clearInterval(timer);
  timer = undefined;
  if (!visible) return;
  void load();
  void nextTick(() => {
    if (jumpPending) scrollToLine();
    else if (scroller.value) scroller.value.scrollTop = savedScroll;
  });
  // ponytail: one stat per visible file every 1.5 s; switch to a daemon `notify` watcher with a `file.changed` event if many files are open at once.
  timer = window.setInterval(load, POLL_MS);
}

const subscription = props.params.api.onDidVisibilityChange((e) => setPolling(e.isVisible));
setPolling(props.params.api.isVisible);
watch(file, () => void nextTick(scrollToLine));
onUnmounted(() => {
  subscription.dispose();
  clearInterval(timer);
});
</script>

<template>
  <div class="file-pane">
    <header class="path" :title="file.path">{{ file.path }}</header>
    <p v-if="error" class="error">{{ error }}</p>
    <div v-else ref="scroller" class="scroller" @scroll="rememberScroll">
      <div class="body">
        <div class="gutter"><span v-for="n in lineCount" :key="n">{{ n }}</span></div>
        <div class="code-wrap">
          <div v-if="file.line" class="mark" :style="{ top: `${(file.line - 1) * LINE_HEIGHT}px` }"></div>
          <pre class="code"><code v-html="html"></code></pre>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.file-pane { position: absolute; inset: 0; display: flex; flex-direction: column; background: var(--bg); color: var(--text); }
.path { padding: 4px 10px; font-size: 12px; color: var(--muted); border-bottom: 1px solid var(--border); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.error { padding: 12px; color: var(--danger); }
.scroller { flex: 1; overflow: auto; }
.body { display: flex; min-width: max-content; font: 12px/18px ui-monospace, SFMono-Regular, Menlo, monospace; }
.gutter { position: sticky; left: 0; display: flex; flex-direction: column; padding: 0 8px; text-align: right; color: var(--muted); background: var(--bg); user-select: none; }
.code-wrap { position: relative; flex: 1; }
.mark { position: absolute; left: 0; right: 0; height: 18px; background: color-mix(in srgb, var(--accent) 18%, transparent); pointer-events: none; }
.code { margin: 0; padding: 0 12px; font: inherit; white-space: pre; }
.code :deep(.hljs-keyword), .code :deep(.hljs-literal), .code :deep(.hljs-selector-tag) { color: var(--hl-keyword); }
.code :deep(.hljs-string), .code :deep(.hljs-regexp) { color: var(--hl-string); }
.code :deep(.hljs-comment), .code :deep(.hljs-quote), .code :deep(.hljs-meta) { color: var(--hl-comment); font-style: italic; }
.code :deep(.hljs-number), .code :deep(.hljs-symbol) { color: var(--hl-number); }
.code :deep(.hljs-title), .code :deep(.hljs-section) { color: var(--hl-title); }
.code :deep(.hljs-type), .code :deep(.hljs-built_in) { color: var(--hl-type); }
.code :deep(.hljs-attr), .code :deep(.hljs-attribute), .code :deep(.hljs-variable), .code :deep(.hljs-tag), .code :deep(.hljs-name) { color: var(--hl-attr); }
</style>
