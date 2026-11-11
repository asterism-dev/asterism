import { reactive, ref } from 'vue';
import { read, write } from './dock/storage';

const WIDTH_KEY = 'asterism.fileTree.width';
const openKey = (taskId: number) => `asterism.fileTree.${taskId}`;

export const TREE_WIDTH = { initial: 260, min: 180, max: 600 };

const clamp = (width: number) =>
  Math.round(Math.min(TREE_WIDTH.max, Math.max(TREE_WIDTH.min, width)));

const stored = Number(read(WIDTH_KEY));
export const treeWidth = ref(
  Number.isFinite(stored) && stored > 0 ? clamp(stored) : TREE_WIDTH.initial,
);

export function setTreeWidth(width: number) {
  treeWidth.value = clamp(width);
  write(WIDTH_KEY, String(treeWidth.value));
}

const open = reactive<Record<number, boolean>>({});

export function isTreeOpen(taskId: number): boolean {
  return (open[taskId] ??= read(openKey(taskId)) === '1');
}

export function setTreeOpen(taskId: number, value: boolean) {
  open[taskId] = value;
  write(openKey(taskId), value ? '1' : '0');
}
