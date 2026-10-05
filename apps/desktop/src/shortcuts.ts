const isMac = navigator.userAgent.includes('Mac');

export type AppShortcut = 'n' | 'j' | 'left' | 'right';

/**
 * New task (`n`), next waiting session (`j`) and toggling Projects / Diff (`b` / Alt+`b`):
 * Cmd on macOS, Ctrl+Shift elsewhere.
 */
export function appShortcut(e: KeyboardEvent): AppShortcut | null {
  if (!(isMac ? e.metaKey : e.ctrlKey && e.shiftKey)) return null;
  // e.code, because Alt changes e.key on macOS (Alt+B types "∫").
  if (e.code === 'KeyB') return e.altKey ? 'right' : 'left';
  const key = e.key.toLowerCase();
  return key === 'n' || key === 'j' ? key : null;
}
