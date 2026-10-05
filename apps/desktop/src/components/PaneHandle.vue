<script setup lang="ts">
const props = defineProps<{ width: number; side: 'left' | 'right' }>();
const emit = defineEmits<{ resize: [width: number]; reset: [] }>();

function start(e: PointerEvent) {
  const handle = e.currentTarget as HTMLElement;
  handle.setPointerCapture(e.pointerId);
  const startX = e.clientX;
  const startWidth = props.width;
  const move = (ev: PointerEvent) => {
    const delta = ev.clientX - startX;
    emit('resize', startWidth + (props.side === 'left' ? delta : -delta));
  };
  const end = () => {
    handle.removeEventListener('pointermove', move);
    handle.removeEventListener('pointerup', end);
    document.body.classList.remove('resizing');
  };
  handle.addEventListener('pointermove', move);
  handle.addEventListener('pointerup', end);
  document.body.classList.add('resizing');
}
</script>

<template>
  <div class="pane-handle" role="separator" aria-orientation="vertical" @pointerdown.prevent="start" @dblclick="emit('reset')"></div>
</template>

<style scoped>
.pane-handle { width: 5px; margin: 0 -2px; cursor: col-resize; z-index: 5; position: relative; }
.pane-handle:hover { background: var(--accent); opacity: 0.4; }
</style>
