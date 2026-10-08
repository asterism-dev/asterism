<script setup lang="ts">
import { computed } from 'vue';
import { shortcutHint, type ActionId } from '../shortcuts';

const props = defineProps<{ action: ActionId }>();
const text = computed(() => shortcutHint(props.action));
</script>

<template>
  <kbd v-if="text" class="shortcut">{{ text }}</kbd>
</template>

<style scoped>
.shortcut {
  font: inherit;
  font-size: 11px;
  padding: 0 4px;
  margin-left: 6px;
  border-radius: 3px;
  background: rgba(127, 127, 127, 0.2);
  opacity: 0.8;
}
</style>
