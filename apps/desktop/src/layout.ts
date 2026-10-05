import { reactive, watch } from 'vue';

export type RightPanel = 'diff' | 'activity';

export interface Layout {
  leftWidth: number;
  leftOpen: boolean;
  rightWidth: number;
  rightOpen: boolean;
  rightPanel: RightPanel;
}

export const LAYOUT_KEY = 'asterism.layout';
export const LEFT = { initial: 260, min: 180, max: 480 };
export const RIGHT = { initial: 420, min: 280, max: 900 };

export const clampWidth = (width: number, limits: { min: number; max: number }) =>
  Math.round(Math.min(limits.max, Math.max(limits.min, width)));

export function parseLayout(raw: string | null): Layout {
  const fallback: Layout = { leftWidth: LEFT.initial, leftOpen: true, rightWidth: RIGHT.initial, rightOpen: false, rightPanel: 'diff' };
  try {
    const v = JSON.parse(raw ?? '{}') as Partial<Layout>;
    return {
      leftWidth: typeof v.leftWidth === 'number' ? clampWidth(v.leftWidth, LEFT) : fallback.leftWidth,
      leftOpen: typeof v.leftOpen === 'boolean' ? v.leftOpen : fallback.leftOpen,
      rightWidth: typeof v.rightWidth === 'number' ? clampWidth(v.rightWidth, RIGHT) : fallback.rightWidth,
      rightOpen: typeof v.rightOpen === 'boolean' ? v.rightOpen : fallback.rightOpen,
      rightPanel: v.rightPanel === 'activity' ? 'activity' : 'diff',
    };
  } catch {
    return fallback;
  }
}

function stored(): string | null {
  try {
    return localStorage.getItem(LAYOUT_KEY);
  } catch {
    return null;
  }
}

export const layout = reactive<Layout>(parseLayout(stored()));

watch(layout, (value) => {
  try {
    localStorage.setItem(LAYOUT_KEY, JSON.stringify(value));
  } catch {
    // Storage can be unavailable; the layout still applies for this run.
  }
});

/** Shows `panel` on the right, or closes the column when it is already showing. */
export function toggleRightPanel(panel: RightPanel) {
  if (layout.rightOpen && layout.rightPanel === panel) {
    layout.rightOpen = false;
  } else {
    layout.rightPanel = panel;
    layout.rightOpen = true;
  }
}
