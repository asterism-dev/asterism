<script setup lang="ts">
import { computed } from 'vue';
import { state } from '../store';
import { paired, revoked } from './connection';
import PairScreen from './PairScreen.vue';

const hostname = computed(() => ('hello' in state.node ? state.node.hello.hostname : ''));
const reload = () => window.location.reload();
</script>

<template>
  <PairScreen v-if="!paired" :revoked="revoked" />
  <div v-else class="mobile">
    <div v-if="state.node.state === 'disconnected'" class="banner">Disconnected, reconnecting…</div>
    <div v-else-if="state.node.state === 'connecting'" class="banner">Connecting…</div>
    <div v-else-if="state.node.state === 'incompatible'" class="banner error">
      {{ state.node.message }} <button @click="reload">Reload</button>
    </div>
    <main class="body">
      <p v-if="hostname" class="empty">Connected to {{ hostname }}</p>
    </main>
    <div class="toasts">
      <div v-for="t in state.toasts" :key="t.id" class="toast">{{ t.message }}</div>
    </div>
  </div>
</template>

<style scoped>
.mobile {
  display: flex;
  flex-direction: column;
  height: 100dvh;
  padding: env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom)
    env(safe-area-inset-left);
}
.body {
  flex: 1;
  min-height: 0;
}
</style>
