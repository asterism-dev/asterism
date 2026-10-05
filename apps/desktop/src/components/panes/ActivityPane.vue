<script setup lang="ts">
import ActivityView from '../ActivityView.vue';

defineProps<{ params: unknown }>();
</script>

<template>
  <div class="pane"><ActivityView /></div>
</template>
