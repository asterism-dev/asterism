<script setup lang="ts">
import { Kbd } from '@asterism-dev/lumen';
import { computed } from 'vue';
import { shortcutHint, type ActionId } from '../shortcuts';

const props = defineProps<{ action: ActionId }>();
const text = computed(() => shortcutHint(props.action));
</script>

<template>
  <Kbd v-if="text" class="shortcut">{{ text }}</Kbd>
</template>

<style scoped>
.shortcut {
  margin-left: 6px;
}
</style>
