import type { RemoteInterface, RemoteServe, RemoteStatus } from './types';

export function interfaceLabel(i: RemoteInterface): string {
  if (i.tailscale) return `Tailscale · ${i.address}`;
  if (i.address === '0.0.0.0') return `All interfaces · ${i.address}`;
  return `${i.name} · ${i.address}`;
}

export function bindWarning(bind: string): string | null {
  return bind === '0.0.0.0' || bind === '::'
    ? 'Listens on every network this Mac joins, not only your tailnet.'
    : null;
}

export function statusLine(s: RemoteStatus): string {
  if (s.state === 'listening') return `Listening on ${s.address ?? ''}`;
  if (s.state === 'error') return `Not listening: ${s.error ?? 'unknown error'}`;
  return 'Off';
}

export const SERVE_OPTIONS: { value: RemoteServe; label: string }[] = [
  { value: 'off', label: 'Off' },
  { value: 'managed', label: 'Run it for me' },
  { value: 'manual', label: "I'll run it myself" },
];

/** The command that undoes `serve_command`, e.g. `tailscale serve --https=8443 off`. */
export function serveOffCommand(serveCommand: string): string {
  return serveCommand.replace(/ --bg (--https=\d+) \S+$/, ' $1 off');
}
