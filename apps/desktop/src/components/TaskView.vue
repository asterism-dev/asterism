<script setup lang="ts">
import { ask } from '@tauri-apps/plugin-dialog';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { computed } from 'vue';
import { api, errorMessage } from '../api';
import { activeTab, addSession, moveTab, showMenu, state, taskSessions, toast, type Tab } from '../store';
import type { Session, SessionKind, Task } from '../types';
import DiffView from './DiffView.vue';
import TerminalPane from './TerminalPane.vue';

const props = defineProps<{ task: Task }>();

const sessions = computed(() => taskSessions(state, props.task.id));
const agents = computed(() => ('hello' in state.node ? state.node.hello.agents.filter((a) => a.available) : []));
const tab = computed(() => activeTab(state, props.task.id));
const activeSession = computed(() => sessions.value.find((s) => s.id === tab.value) ?? null);
const report = (e: unknown) => toast(errorMessage(e));
let dragged: number | null = null;
// Sessions being closed; a second click during the confirm or the daemon's grace period is ignored.
const closing = new Set<number>();

function select(next: Tab) {
  state.selectedTab[props.task.id] = next;
}

function start(kind: SessionKind) {
  api.startSession(props.task.id, kind).then((s) => addSession(state, s)).catch(report);
}

function label(s: Session): string {
  if (s.kind.type === 'agent') return s.kind.name;
  if (s.kind.type === 'shell') return 'shell';
  return s.kind.argv[0] ?? 'command';
}

function drop(target: number) {
  if (dragged !== null && dragged !== target) moveTab(state, props.task.id, dragged, target);
  dragged = null;
}

async function close(s: Session) {
  if (closing.has(s.id)) return;
  closing.add(s.id);
  try {
    if (s.kind.type === 'agent' && s.status !== 'exited') {
      const stop = await ask(`Stop the running ${label(s)} session and close it?`, { title: 'Close session', kind: 'warning' });
      if (!stop) return;
    }
    await api.removeSession(s.id);
  } catch (e) {
    report(e);
  } finally {
    closing.delete(s.id);
  }
}

function tabMenu(e: MouseEvent, s: Session) {
  showMenu(e, [{ label: 'Close session', danger: true, action: () => close(s) }]);
}
</script>

<template>
  <section class="task-view">
    <header class="task-header">
      <div class="title">
        <h1>{{ task.title }}</h1>
        <span class="branch">{{ task.branch }} ← {{ task.base_branch }}</span>
      </div>
      <div class="toolbar">
        <button v-for="a in agents" :key="a.name" @click="start({ type: 'agent', name: a.name })">New {{ a.name }}</button>
        <button @click="start({ type: 'shell' })">New shell</button>
        <button @click="revealItemInDir(task.worktree_path).catch(report)">Reveal worktree</button>
      </div>
    </header>
    <nav class="tabs">
      <div
        v-for="s in sessions"
        :key="s.id"
        class="tab"
        role="tab"
        :aria-selected="tab === s.id"
        :class="{ active: tab === s.id, exited: s.status === 'exited' }"
        draggable="true"
        @dragstart="dragged = s.id; $event.dataTransfer?.setData('text/plain', String(s.id))"
        @dragover.prevent
        @drop="drop(s.id)"
        @contextmenu="tabMenu($event, s)"
      >
        <button class="tab-label" @click="select(s.id)" @keydown.delete="close(s)">
          <span class="dot" :class="s.status"></span>{{ label(s) }}
        </button>
        <button class="close" aria-label="Close session" title="Close session" @click.stop="close(s)">×</button>
      </div>
      <button class="tab" :class="{ active: tab === 'diff' }" @click="select('diff')">Diff</button>
    </nav>
    <div class="tab-body">
      <DiffView v-if="tab === 'diff'" :task="task" />
      <TerminalPane
        v-else-if="activeSession"
        :key="`${activeSession.id}-${activeSession.status === 'exited'}`"
        :session-id="activeSession.id"
        :live="activeSession.status !== 'exited'"
      />
    </div>
  </section>
</template>

<style scoped>
.task-view { display: flex; flex-direction: column; flex: 1; min-height: 0; }
.task-header { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 10px 14px; border-bottom: 1px solid var(--border); background: var(--panel); }
.title h1 { font-size: 15px; margin: 0; }
.branch { color: var(--muted); font-family: ui-monospace, Menlo, monospace; font-size: 12px; }
.toolbar { display: flex; gap: 6px; flex-wrap: wrap; }
.tabs { display: flex; gap: 2px; padding: 6px 10px 0; background: var(--panel); border-bottom: 1px solid var(--border); }
.tab { border: 0; border-radius: 6px 6px 0 0; display: flex; align-items: center; gap: 6px; background: transparent; }
.tab[role="tab"] { padding: 0; }
.tab.active { background: var(--select); }
.tab.exited { color: var(--muted); }
.tab-body { position: relative; flex: 1; min-height: 0; }
.tab-label { border: 0; background: transparent; display: flex; align-items: center; gap: 6px; padding: 3px 4px 3px 9px; cursor: pointer; color: inherit; font-size: inherit; font-family: inherit; }
.tab-label:focus { outline: 1px solid var(--border); }
.close { border: 0; background: transparent; padding: 0 2px; border-radius: 4px; opacity: 0; cursor: pointer; }
.tab:hover .close, .tab.active .close, .tab:focus-within .close { opacity: 0.6; }
.close:hover { opacity: 1; background: var(--border); }
</style>
