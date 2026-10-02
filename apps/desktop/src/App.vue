<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { onMounted, ref } from 'vue';

const status = ref('connecting');

onMounted(async () => {
  await listen<{ state: string }>('node-status', (e) => (status.value = e.payload.state));
  status.value = (await invoke<{ state: string }>('node_status')).state;
});
</script>

<template>
  <p id="node-status">{{ status }}</p>
</template>
