import { getCurrentWindow } from '@tauri-apps/api/window';
import { ref } from 'vue';

export type ThemeChoice = 'system' | 'light' | 'dark';
export type Theme = 'light' | 'dark';

const STORAGE_KEY = 'asterism.theme';
const systemQuery = '(prefers-color-scheme: dark)';

export function parseTheme(value: string | null): ThemeChoice {
  return value === 'light' || value === 'dark' ? value : 'system';
}

export function resolveTheme(choice: ThemeChoice, systemDark: boolean): Theme {
  return choice === 'system' ? (systemDark ? 'dark' : 'light') : choice;
}

export const themeChoice = ref<ThemeChoice>('system');
export const activeTheme = ref<Theme>('light');

function storedChoice(): ThemeChoice {
  try {
    return parseTheme(localStorage.getItem(STORAGE_KEY));
  } catch {
    return 'system';
  }
}

function apply() {
  activeTheme.value = resolveTheme(themeChoice.value, window.matchMedia(systemQuery).matches);
  document.documentElement.dataset.theme = activeTheme.value;
  // Keeps the native title bar in step; `null` lets it follow the OS for "system".
  getCurrentWindow().setTheme(themeChoice.value === 'system' ? null : themeChoice.value).catch(() => {});
}

export function setTheme(choice: ThemeChoice) {
  themeChoice.value = choice;
  try {
    localStorage.setItem(STORAGE_KEY, choice);
  } catch {
    // Storage can be unavailable; the choice still applies for this run.
  }
  apply();
}

export function initTheme() {
  themeChoice.value = storedChoice();
  apply();
  window.matchMedia(systemQuery).addEventListener('change', () => {
    if (themeChoice.value === 'system') apply();
  });
}
