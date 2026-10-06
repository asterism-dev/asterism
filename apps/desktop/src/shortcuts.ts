const isMac = navigator.userAgent.includes('Mac');

export type AppShortcut = 'n' | 'j' | 'f' | 'left' | 'right';

/**
 * New task (`n`), next waiting session (`j`), sidebar search (`f`) and toggling the sidebar / Diff (`b` / Alt+`b`):
 * Cmd on macOS, Ctrl+Shift elsewhere.
 */
export function appShortcut(e: KeyboardEvent): AppShortcut | null {
  if (!(isMac ? e.metaKey : e.ctrlKey && e.shiftKey)) return null;
  // e.code, because Alt changes e.key on macOS (Alt+B types "∫").
  if (e.code === 'KeyB') return e.altKey ? 'right' : 'left';
  const key = e.key.toLowerCase();
  return key === 'n' || key === 'j' || key === 'f' ? key : null;
}
