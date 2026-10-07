<script setup lang="ts">
import { state, type MenuItem } from '../store';

function run(item: MenuItem) {
  state.menu = null;
  item.action();
}
</script>

<template>
  <div
    v-if="state.menu"
    class="menu"
    :style="{ left: `${state.menu.x}px`, top: `${state.menu.y}px` }"
    @click.stop
  >
    <button
      v-for="item in state.menu.items"
      :key="item.label"
      :class="{ danger: item.danger }"
      @click="run(item)"
    >
      <span class="check">{{ item.checked ? '✓' : '' }}</span
      >{{ item.label }}
    </button>
  </div>
</template>

<style scoped>
.check {
  display: inline-block;
  width: 1.2em;
}
</style>
