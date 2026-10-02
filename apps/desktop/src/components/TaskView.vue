<script setup lang="ts">
import type { Task } from '../types';

defineProps<{ task: Task }>();
</script>

<template>
  <section class="task-view">
    <h1>{{ task.title }}</h1>
  </section>
</template>
