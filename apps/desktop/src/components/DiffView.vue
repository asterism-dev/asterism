<script setup lang="ts">
import type { Task } from '../types';

defineProps<{ task: Task }>();
</script>

<template>
  <div class="diff-view"></div>
</template>
