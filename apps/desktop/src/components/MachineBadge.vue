<script setup lang="ts">
import { Badge } from '@asterism-dev/lumen';

defineProps<{ name: string; ready: boolean }>();
</script>

<template>
  <Badge :tone="ready ? 'muted' : 'warning'" :title="ready ? name : `${name} is offline`"
    >{{ name }}<template v-if="!ready"> · offline</template></Badge
  >
</template>
