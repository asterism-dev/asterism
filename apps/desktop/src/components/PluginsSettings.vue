<script setup lang="ts">
import { Tabs } from '@asterism-dev/lumen';
import { ref } from 'vue';
import { state } from '../store';
import DiscoverTab from './plugins/DiscoverTab.vue';
import InstalledTab from './plugins/InstalledTab.vue';
import StoresTab from './plugins/StoresTab.vue';

const tab = ref('installed');
const TABS = [
  { value: 'installed', label: 'Installed' },
  { value: 'discover', label: 'Discover' },
  { value: 'stores', label: 'Stores' },
];

const configure = (name: string) => (state.pluginSettingsRequest = name);
</script>

<template>
  <section>
    <Tabs v-model="tab" class="tabs" :tabs="TABS">
      <template #installed><InstalledTab /></template>
      <template #discover><DiscoverTab @configure="configure" /></template>
      <template #stores><StoresTab /></template>
    </Tabs>
  </section>
</template>

<style scoped>
.tabs :deep(.lm-tabs__list) {
  margin-bottom: 12px;
}
</style>
