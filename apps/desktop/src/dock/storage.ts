export function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage can be unavailable; the layout still applies for this run.
  }
}

export function remove(key: string) {
  try {
    localStorage.removeItem(key);
  } catch {
    // Nothing stored means nothing to remove.
  }
}

export function keys(): string[] {
  try {
    return Object.keys(localStorage);
  } catch {
    return [];
  }
}
