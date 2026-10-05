<script setup lang="ts">
import Sidebar from '../Sidebar.vue';

defineProps<{ params: unknown }>();
</script>

<template>
  <div class="pane"><Sidebar /></div>
</template>
