import { watch } from 'vue';
import { api } from './api';
import {
  handleMessage,
  panelEvent,
  THEME_VARS,
  type BridgeDeps,
  type PanelContext,
  type ThemeInfo,
} from './pluginBridge';
import { selectSession, state, taskSessions } from './store';
import { activeTheme } from './theme';
import type { NodeEvent } from './types';

const frames = new Map<Window, PanelContext>();

export function pluginUrl(plugin: string, path: string): string {
  const base = navigator.userAgent.includes('Windows')
    ? 'http://asterism-plugin.localhost'
    : 'asterism-plugin://localhost';
  return `${base}/${plugin}/${path}`;
}

function theme(): ThemeInfo {
  const style = getComputedStyle(document.documentElement);
  return {
    name: activeTheme.value,
    vars: Object.fromEntries(THEME_VARS.map((v) => [v, style.getPropertyValue(v).trim()])),
  };
}

const deps: BridgeDeps = {
  sessions: (taskId) => taskSessions(state, taskId),
  subagents: (sessionId) => api.subagents(sessionId),
  focus: (session) => selectSession(state, session),
  theme,
};

export function registerFrame(win: Window, ctx: PanelContext): () => void {
  frames.set(win, ctx);
  return () => frames.delete(win);
}

export function forwardEvent(event: NodeEvent) {
  for (const [win, ctx] of frames) {
    if (!ctx.subscribed) continue;
    const message = panelEvent(event, ctx.taskId, state.sessions);
    if (message) win.postMessage(message, '*');
  }
}

export function initPluginFrames() {
  // Sandboxed frames have an opaque origin, hence '*'; the frame map is the trust check.
  window.addEventListener('message', async (e) => {
    const win = e.source as Window | null;
    const ctx = win && frames.get(win);
    if (!win || !ctx) return;
    const response = await handleMessage(e.data, ctx, deps);
    if (response) win.postMessage(response, '*');
  });
  watch(
    activeTheme,
    () => {
      for (const win of frames.keys()) win.postMessage({ event: 'theme', data: theme() }, '*');
    },
    { flush: 'post' },
  );
}
