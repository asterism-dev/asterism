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

/** A toast message the moment a delete activity finishes with failed steps; null otherwise. */
export function deleteFailures(prev: Activity | undefined, next: Activity): string | null {
  if (next.kind !== 'delete_tasks' || !next.finished_at || prev?.finished_at) return null;
  const failed = next.steps.filter((s) => s.status === 'failed');
  if (!failed.length) return null;
  return `${next.title} with problems — ${failed.map((s) => `${s.label}: ${s.detail}`).join('; ')}`;
}
