import { ref } from 'vue';
import { keys, remove } from './storage';
import { staleWorkspaceKeys, WORKSPACE_PREFIX } from './workspaceModel';

import type { Placement } from './workspaceModel';

export type { Placement };

// A session's placement is chosen before the daemon returns its id, so it waits here until its panel is added.
const placements = new Map<number, Placement>();
export const workspaceEpoch = ref(0);

export function placeNext(sessionId: number, placement: Placement) {
  placements.set(sessionId, placement);
}

export function takePlacement(sessionId: number): Placement | undefined {
  const placement = placements.get(sessionId);
  placements.delete(sessionId);
  return placement;
}

export function clearWorkspaces() {
  keys().filter((key) => key.startsWith(WORKSPACE_PREFIX)).forEach(remove);
  workspaceEpoch.value++;
}

export function pruneWorkspaces(taskIds: number[]) {
  staleWorkspaceKeys(keys(), taskIds).forEach(remove);
}
