<script setup lang="ts">
import { Menu } from '@asterism-dev/lumen';
import { state } from '../store';

function onOpen(open: boolean) {
  if (!open) state.menu = null;
}
</script>

<template>
  <!-- showMenu opens at the pointer, so an invisible anchor there stands in for the trigger -->
  <Menu
    v-if="state.menu"
    :key="`${state.menu.x},${state.menu.y}`"
    :open="true"
    :items="state.menu.items"
    @update:open="onOpen"
  >
    <span class="menu-anchor" :style="{ left: `${state.menu.x}px`, top: `${state.menu.y}px` }" />
  </Menu>
</template>

<style scoped>
.menu-anchor {
  position: fixed;
  width: 0;
  height: 0;
  pointer-events: none;
}
</style>
