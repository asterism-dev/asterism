import type { Activity } from './types';

const MAX_ACTIVITIES = 50;

export function upsertActivity(list: Activity[], a: Activity): Activity[] {
  const i = list.findIndex((x) => x.id === a.id);
  if (i >= 0) return list.map((x, j) => (j === i ? a : x));
  return [a, ...list].slice(0, MAX_ACTIVITIES);
}

export function formatDiffCount(n: number): string {
  if (n <= 0) return '';
  if (n < 1000) return `+${n}`;
  return `+${Number((n / 1000).toFixed(1))}k`;
}

export function anyRunning(list: Activity[]): boolean {
  return list.some((a) => a.status === 'running');
}

export function activityFor(list: Activity[], token: string): Activity | undefined {
  return list.find((a) => a.token === token);
}
