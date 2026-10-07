<script setup lang="ts">
import { openUrl } from '@tauri-apps/plugin-opener';
import { computed } from 'vue';
import { errorMessage } from '../api';
import { prBadge } from '../prBadge';
import { toast } from '../store';
import type { PullRequest } from '../types';

const props = defineProps<{ pr: PullRequest }>();
const badge = computed(() => prBadge(props.pr));

function open() {
  openUrl(props.pr.url).catch((e) => toast(errorMessage(e)));
}
</script>

<template>
  <a class="pr-badge" :class="`pr-${badge.tone}`" :href="pr.url" :title="badge.tooltip" @click.prevent.stop="open">{{ badge.label }}</a>
</template>
