<script setup lang="ts">
import { CircleDot } from '@lucide/vue';

withDefaults(defineProps<{ id: string; size?: number }>(), { size: 14 });
</script>

<template>
  <svg v-if="id.startsWith('github')" :width="size" :height="size" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" class="provider-icon">
    <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
  </svg>
  <!-- Simplified Linear mark: a disc crossed by diagonal gaps. -->
  <svg v-else-if="id.startsWith('linear')" :width="size" :height="size" viewBox="0 0 16 16" aria-hidden="true" class="provider-icon">
    <defs><clipPath id="linear-disc"><circle cx="8" cy="8" r="8" /></clipPath></defs>
    <g clip-path="url(#linear-disc)" fill="currentColor">
      <path d="M-2 10 L6 18 L4 20 L-4 12 Z M-2 5 L11 18 L9.5 19.5 L-3.5 6.5 Z M0 1 L15 16 L18 13 L3 -2 Z" />
    </g>
  </svg>
  <CircleDot v-else :size="size" class="provider-icon" />
</template>

<style scoped>
.provider-icon { display: inline-block; vertical-align: -2px; flex: none; }
</style>
