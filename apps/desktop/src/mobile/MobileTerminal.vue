<script setup lang="ts">
import { ref } from 'vue';
import { api } from '../api';
import TerminalPane from '../components/TerminalPane.vue';
import type { Session } from '../types';

const props = defineProps<{ session: Session }>();
const showKeys = ref(true);
// Keys the iOS on-screen keyboard lacks; a hardware keyboard sends them directly.
const KEYS = [
  { label: 'Esc', data: '\x1b' },
  { label: 'Tab', data: '\t' },
  { label: '^C', data: '\x03' },
  { label: '←', data: '\x1b[D' },
  { label: '↑', data: '\x1b[A' },
  { label: '↓', data: '\x1b[B' },
  { label: '→', data: '\x1b[C' },
  { label: '⏎', data: '\r' },
];
</script>

<template>
  <div class="mobile-terminal">
    <div class="screen">
      <TerminalPane
        :key="session.id"
        :session-id="session.id"
        :task-id="session.task_id"
        :live="session.status !== 'exited'"
      />
    </div>
    <div v-if="session.status !== 'exited'" class="keys">
      <template v-if="showKeys">
        <button
          v-for="k in KEYS"
          :key="k.label"
          @pointerdown.prevent
          @click="api.send(props.session.id, k.data)"
        >
          {{ k.label }}
        </button>
      </template>
      <button class="toggle" @click="showKeys = !showKeys">{{ showKeys ? '⌄' : '⌃' }}</button>
    </div>
  </div>
</template>

<style scoped>
.mobile-terminal {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.screen {
  position: relative;
  flex: 1;
  min-height: 0;
}
.keys {
  display: flex;
  gap: 4px;
  padding: 4px;
  overflow-x: auto;
}
.keys button {
  min-width: 44px;
  min-height: 44px;
}
.toggle {
  margin-left: auto;
}
</style>
