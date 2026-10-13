export async function ask(message: string): Promise<boolean> {
  return window.confirm(message);
}

// Folder pickers have no browser counterpart; callers treat null as "cancelled".
export async function open(): Promise<null> {
  return null;
}
