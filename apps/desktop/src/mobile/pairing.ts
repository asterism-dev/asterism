export const TOKEN_KEY = 'asterism.remoteToken';

export function tokenFromHash(hash: string): string | null {
  return /^#token=([A-Za-z0-9_-]+)$/.exec(hash)?.[1] ?? null;
}

export function loadToken(storage: Pick<Storage, 'getItem'>): string | null {
  try {
    return storage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

export function clearToken(storage: Pick<Storage, 'removeItem'>) {
  try {
    storage.removeItem(TOKEN_KEY);
  } catch {
    // Storage can be unavailable; the token is dropped from memory anyway.
  }
}

/** Takes the token from a pairing URL into storage, so it never stays in the address bar or history. */
export function adoptPairing(
  loc: Pick<Location, 'hash' | 'pathname' | 'search'>,
  history: Pick<History, 'replaceState'>,
  storage: Pick<Storage, 'getItem' | 'setItem'>,
): string | null {
  const fresh = tokenFromHash(loc.hash);
  if (!fresh) return loadToken(storage);
  try {
    storage.setItem(TOKEN_KEY, fresh);
  } catch {
    // Without storage the device stays paired only until the page reloads.
  }
  history.replaceState(null, '', `${loc.pathname}${loc.search}#/`);
  return fresh;
}

export function socketUrl(loc: Pick<Location, 'protocol' | 'host'>): string {
  return `${loc.protocol === 'https:' ? 'wss' : 'ws'}://${loc.host}/ws`;
}
