<script setup lang="ts">
import { Button } from '@asterism-dev/lumen';
import { ref } from 'vue';
import { api } from '../api';
import TerminalPane from '../components/TerminalPane.vue';
import type { Session } from '../types';

const props = defineProps<{ session: Session }>();
const showKeys = ref(true);
// Keys the iOS on-screen keyboard lacks; a hardware keyboard sends them directly.
const KEYS = [
  { label: 'Esc', name: 'Escape', data: '\x1b' },
  { label: 'Tab', name: 'Tab', data: '\t' },
  { label: '^C', name: 'Control C', data: '\x03' },
  { label: '←', name: 'Left', data: '\x1b[D' },
  { label: '↑', name: 'Up', data: '\x1b[A' },
  { label: '↓', name: 'Down', data: '\x1b[B' },
  { label: '→', name: 'Right', data: '\x1b[C' },
  { label: '⏎', name: 'Enter', data: '\r' },
];
</script>

<template>
  <div class="mobile-terminal">
    <div class="screen">
      <TerminalPane
        :key="`${session.id}-${session.status === 'exited'}`"
        :session-id="session.id"
        :task-id="session.task_id"
        :live="session.status !== 'exited'"
      />
    </div>
    <div v-if="session.status !== 'exited'" class="keys">
      <div v-if="showKeys" class="scroll">
        <Button
          v-for="k in KEYS"
          :key="k.label"
          :aria-label="k.name"
          @pointerdown.prevent
          @click="api.send(props.session.id, k.data)"
        >
          {{ k.label }}
        </Button>
      </div>
      <Button
        variant="ghost"
        class="toggle"
        :aria-label="showKeys ? 'Hide keys' : 'Show keys'"
        @click="showKeys = !showKeys"
      >
        {{ showKeys ? '⌄' : '⌃' }}
      </Button>
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
}
.scroll {
  display: flex;
  flex: 1;
  min-width: 0;
  gap: 4px;
  overflow-x: auto;
}
.keys button {
  flex: none;
  min-width: 44px;
}
/* on a phone all keys must stay visible, so they share the width instead of scrolling */
@media (max-width: 767.98px) {
  .scroll button {
    flex: 1;
    min-width: 0;
    padding: 0;
  }
}
.toggle {
  margin-left: auto;
}
</style>
