<script setup lang="ts">
import { ref } from 'vue';
import DiscoverTab from './plugins/DiscoverTab.vue';
import InstalledTab from './plugins/InstalledTab.vue';
import StoresTab from './plugins/StoresTab.vue';

const tab = ref<'installed' | 'discover' | 'stores'>('installed');
const configureRequest = ref<string | null>(null);
const TABS = [
  { value: 'installed', label: 'Installed' },
  { value: 'discover', label: 'Discover' },
  { value: 'stores', label: 'Stores' },
] as const;

function configure(name: string) {
  tab.value = 'installed';
  configureRequest.value = name;
}
</script>

<template>
  <section>
    <div class="tabs" role="tablist" aria-label="Plugins">
      <button v-for="t in TABS" :key="t.value" role="tab" :aria-selected="tab === t.value" :class="{ active: tab === t.value }" @click="tab = t.value">
        {{ t.label }}
      </button>
    </div>
    <InstalledTab v-if="tab === 'installed'" :configure-request="configureRequest" @configured="configureRequest = null" />
    <DiscoverTab v-else-if="tab === 'discover'" @configure="configure" />
    <StoresTab v-else />
  </section>
</template>

<style scoped>
.tabs { display: flex; gap: 6px; margin-bottom: 12px; }
.tabs button { border: 0; }
.tabs button.active { background: var(--select); }
</style>
