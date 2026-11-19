<script setup lang="ts">
import { Button, TextInput } from '@asterism-dev/lumen';
import { ask } from '@tauri-apps/plugin-dialog';
import { ref } from 'vue';
import { api, errorMessage } from '../api';
import { machineStatusLine } from '../machines';
import { state, toast } from '../store';
import type { Machine } from '../types';

const name = ref('');
const destination = ref('');
const busy = ref(false);
const testResult = ref<{ ok: boolean; text: string } | null>(null);
const report = (e: unknown) => toast(errorMessage(e));

const canSubmit = () => !!name.value.trim() && !!destination.value.trim() && !busy.value;

async function test() {
  busy.value = true;
  testResult.value = null;
  try {
    const { platform } = await api.testMachine(destination.value.trim());
    testResult.value = { ok: true, text: `Connected · ${platform}` };
  } catch (e) {
    testResult.value = { ok: false, text: errorMessage(e) };
  } finally {
    busy.value = false;
  }
}

async function add() {
  if (!canSubmit()) return;
  busy.value = true;
  try {
    await api.addMachine(name.value.trim(), destination.value.trim());
    name.value = '';
    destination.value = '';
    testResult.value = null;
  } catch (e) {
    report(e);
  } finally {
    busy.value = false;
  }
}

async function remove(m: Machine) {
  const message = `Remove ${m.name}? Asterism disconnects from it.`;
  if (!(await ask(message, { title: 'Remove machine', kind: 'warning' }))) return;
  api.removeMachine(m.id).catch(report);
}
</script>

<template>
  <div class="agent-settings machines-settings">
    <section>
      <h3>Machines</h3>
      <p class="hint">
        Remote machines Asterism reaches over SSH. Key-based login must work without a prompt.
      </p>
      <div v-for="m in state.machines" :key="m.id" class="settings-row machine-row">
        <span>{{ m.name }}</span>
        <span class="hint destination">{{ m.destination }}</span>
        <span class="hint status">{{ machineStatusLine(m) }}</span>
        <Button
          v-if="m.status.state === 'blocked' || m.status.state === 'offline'"
          @click="api.retryMachine(m.id).catch(report)"
          >Retry</Button
        >
        <Button variant="danger" @click="remove(m)">Remove</Button>
      </div>
      <p v-if="!state.machines.length" class="hint">No machines yet.</p>
    </section>
    <section>
      <h3>Add machine</h3>
      <form class="settings-row" @submit.prevent="add">
        <TextInput v-model="name" placeholder="Name" aria-label="Name" />
        <TextInput
          v-model="destination"
          placeholder="user@host or ~/.ssh/config alias"
          spellcheck="false"
          aria-label="SSH destination"
        />
        <Button :disabled="!destination.trim() || busy" @click="test">Test connection</Button>
        <Button type="submit" variant="accent" :disabled="!canSubmit()">Add</Button>
      </form>
      <p v-if="testResult" :class="testResult.ok ? 'muted' : 'error'">{{ testResult.text }}</p>
    </section>
  </div>
</template>

<style scoped>
.machines-settings section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.machines-settings section > * {
  margin: 0;
}
.machine-row {
  align-items: center;
}
.machine-row .status {
  flex: 1;
}
</style>
