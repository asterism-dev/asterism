import type { Machine } from './types';

export function machineStatusLine(m: Machine): string {
  const { state, platform, error, update_available } = m.status;
  switch (state) {
    case 'ready':
      return ['Connected', platform, update_available && 'update available']
        .filter(Boolean)
        .join(' · ');
    case 'connecting':
      return 'Connecting…';
    case 'offline':
      return `Offline, retrying: ${error ?? 'unknown error'}`;
    case 'blocked':
      return `Needs attention: ${error ?? 'unknown error'}`;
  }
}
