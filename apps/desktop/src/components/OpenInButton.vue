<script setup lang="ts">
import { Button } from '@asterism-dev/lumen';
import { ChevronDown, FolderOpen } from '@lucide/vue';
import { computed, onMounted, ref } from 'vue';
import { errorMessage } from '../api';
import { fetchInstalledApps, lastApp, openIn, rememberApp } from '../openIn';
import { showMenu, toast } from '../store';

const props = defineProps<{ path: string }>();

const installed = ref<string[]>([]);
const current = ref('Finder');

let pending: Promise<void> | null = null;
function load() {
  pending ??= fetchInstalledApps()
    .then((apps) => {
      installed.value = apps;
      current.value = lastApp(apps);
    })
    .catch(() => {});
  return pending;
}
onMounted(load);

const available = computed(() => (installed.value.length ? installed.value : ['Finder']));

function open(app: string) {
  openIn(app, props.path).catch((e) => toast(errorMessage(e)));
}

function choose(app: string) {
  current.value = app;
  rememberApp(app);
  open(app);
}

function menu(e: MouseEvent) {
  showMenu(
    e,
    available.value.map((app) => ({
      label: app,
      checked: app === current.value,
      action: () => choose(app),
    })),
  );
}
</script>

<template>
  <div class="open-in">
    <Button variant="ghost" :icon="FolderOpen" :title="`Open in ${current}`" @click="open(current)">
      {{ current }}
    </Button>
    <Button
      variant="ghost"
      size="sm"
      :icon="ChevronDown"
      title="Open in…"
      aria-label="Choose app"
      @click="menu"
    />
  </div>
</template>

<style scoped>
.open-in {
  display: flex;
}
.open-in > * {
  color: var(--muted);
}
</style>
