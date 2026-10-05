<script setup lang="ts">
import type { DockviewPanelApi } from 'dockview-vue';

defineProps<{ params: { api: DockviewPanelApi } }>();
</script>

<template>
  <div class="fixed-tab">{{ params.api.title }}</div>
</template>

<style scoped>
.fixed-tab { padding: 0 10px; display: flex; align-items: center; height: 100%; }
</style>
