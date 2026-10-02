const isMac = navigator.userAgent.includes('Mac');

/** New task (`n`) and next waiting session (`j`): Cmd on macOS, Ctrl+Shift elsewhere. */
export function appShortcut(e: KeyboardEvent): 'n' | 'j' | null {
  if (!(isMac ? e.metaKey : e.ctrlKey && e.shiftKey)) return null;
  const key = e.key.toLowerCase();
  return key === 'n' || key === 'j' ? key : null;
}
