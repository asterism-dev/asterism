export type DetailTab = 'sessions' | 'terminal' | 'review';
export type Route =
  { name: 'list' } | { name: 'task'; taskId: number; tab: DetailTab; sessionId: number | null };

const id = (part?: string) => (part && /^\d+$/.test(part) ? Number(part) : null);

export function parseRoute(hash: string): Route {
  const [kind, taskPart, tabPart, sessionPart] = hash.replace(/^#\/?/, '').split('/');
  const taskId = kind === 't' ? id(taskPart) : null;
  if (taskId === null) return { name: 'list' };
  if (tabPart === 'terminal')
    return { name: 'task', taskId, tab: 'terminal', sessionId: id(sessionPart) };
  return {
    name: 'task',
    taskId,
    tab: tabPart === 'review' ? 'review' : 'sessions',
    sessionId: null,
  };
}

export function formatRoute(route: Route): string {
  if (route.name === 'list') return '#/';
  const base = `#/t/${route.taskId}`;
  if (route.tab === 'sessions') return base;
  if (route.tab === 'review') return `${base}/review`;
  return route.sessionId === null ? `${base}/terminal` : `${base}/terminal/${route.sessionId}`;
}
