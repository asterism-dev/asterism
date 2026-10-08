import { watch } from 'vue';
import { api } from './api';
import {
  frameFor,
  handleMessage,
  panelEvent,
  plainSessions,
  THEME_VARS,
  type BridgeDeps,
  type PanelContext,
  type ThemeInfo,
} from './pluginBridge';
import { selectSession, state, taskSessions } from './store';
import { activeTheme } from './theme';
import type { NodeEvent } from './types';

// Keyed by element: dockview re-attaching an iframe gives it a new contentWindow.
const frames = new Map<HTMLIFrameElement, PanelContext>();

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

// A closed frame or an uncloneable value must never throw out of the app.
function post(win: Window | null, message: unknown) {
  if (!win) return;
  try {
    win.postMessage(message, '*');
  } catch (err) {
    console.warn('plugin panel postMessage failed', err);
  }
}

const deps: BridgeDeps = {
  sessions: (taskId) => plainSessions(taskSessions(state, taskId)),
  subagents: (sessionId) => api.subagents(sessionId),
  focus: (session) => selectSession(state, session),
  theme,
};

export function registerFrame(el: HTMLIFrameElement, ctx: PanelContext): () => void {
  frames.set(el, ctx);
  post(el.contentWindow, { event: 'theme', data: theme() });
  return () => frames.delete(el);
}

// Keeps `subscribed` across reloads: the new document's subscribe may beat this load event.
export function frameLoaded(el: HTMLIFrameElement) {
  if (frames.has(el)) post(el.contentWindow, { event: 'theme', data: theme() });
}

export function forwardEvent(event: NodeEvent) {
  for (const [el, ctx] of frames) {
    if (!ctx.subscribed) continue;
    const message = panelEvent(event, ctx.taskId, state.sessions);
    if (message) post(el.contentWindow, message);
  }
}

export function initPluginFrames() {
  // Sandboxed frames have an opaque origin, hence '*'; the frame map is the trust check.
  window.addEventListener('message', async (e) => {
    const ctx = frameFor(e.source, frames);
    if (!ctx) return;
    const response = await handleMessage(e.data, ctx, deps);
    if (response) post(e.source as Window, response);
  });
  watch(
    activeTheme,
    () => {
      for (const el of frames.keys()) post(el.contentWindow, { event: 'theme', data: theme() });
    },
    { flush: 'post' },
  );
}
