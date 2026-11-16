<script setup lang="ts">
import { Badge } from '@asterism-dev/lumen';
import { openUrl } from '@tauri-apps/plugin-opener';
import { computed } from 'vue';
import { errorMessage } from '../api';
import { prBadge, type BadgeTone } from '../prBadge';
import { toast } from '../store';
import type { PullRequest } from '../types';

const props = defineProps<{ pr: PullRequest }>();
const badge = computed(() => prBadge(props.pr));
const TONES = {
  draft: 'muted',
  pending: 'warning',
  success: 'success',
  failure: 'danger',
  merged: 'merged',
  closed: 'muted',
} as const satisfies Record<BadgeTone, string>;

function open() {
  openUrl(props.pr.url).catch((e) => toast(errorMessage(e)));
}
</script>

<template>
  <Badge
    as="a"
    :tone="TONES[badge.tone]"
    :class="{ closed: badge.tone === 'closed' }"
    :href="pr.url"
    :title="badge.tooltip"
    @click.prevent.stop="open"
    >{{ badge.label }}</Badge
  >
</template>

<style scoped>
.closed {
  text-decoration: line-through;
}
</style>
