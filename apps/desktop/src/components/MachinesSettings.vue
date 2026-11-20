<script setup lang="ts">
import { Button, Dialog, TextInput } from '@asterism-dev/lumen';
import { ask } from '@tauri-apps/plugin-dialog';
import { computed, onMounted, ref } from 'vue';
import { api, errorMessage } from '../api';
import { forgetConfirmText, updateConfirmText } from '../machines';
import { state, toast } from '../store';
import type { Machine } from '../types';
import MachineCard from './MachineCard.vue';
import MachineDetail from './MachineDetail.vue';

const name = ref('');
const destination = ref('');
const busy = ref(false);
const adding = ref(false);
const selectedId = ref<number | null>(null);
const selected = computed(() => state.machines.find((m) => m.id === selectedId.value));
const updating = ref(new Set<number>());
const testResult = ref<{ ok: boolean; text: string } | null>(null);
const report = (e: unknown) => toast(errorMessage(e));

const canSubmit = () => !!name.value.trim() && !!destination.value.trim() && !busy.value;

onMounted(() => {
  api
    .machines()
    .then((machines) => (state.machines = machines))
    .catch(report);
});

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
    adding.value = false;
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

async function update(m: Machine) {
  try {
    const preview = await api.machineUpdatePreview(m.id);
    if (!(await ask(updateConfirmText(preview), { title: 'Update machine', kind: 'warning' }))) {
      return;
    }
    updating.value.add(m.id);
    await api.updateMachine(m.id);
  } catch (e) {
    report(e);
  } finally {
    updating.value.delete(m.id);
  }
}

async function forget(m: Machine) {
  try {
    const tasks = await api.machineTasks(m.id);
    const message = forgetConfirmText(m.name, tasks);
    if (!(await ask(message, { title: 'Forget machine', kind: 'warning' }))) return;
    await api.forgetMachine(m.id);
  } catch (e) {
    report(e);
  }
}
</script>

<template>
  <div class="agent-settings machines-settings">
    <MachineDetail
      v-if="selected"
      :machine="selected"
      :updating="updating.has(selected.id)"
      @back="selectedId = null"
      @update="update(selected)"
      @remove="remove(selected)"
      @forget="forget(selected)"
    />
    <section v-else>
      <header>
        <h3>Machines</h3>
        <Button variant="accent" @click="adding = true">Add machine</Button>
      </header>
      <p class="hint">
        Remote machines Asterism reaches over SSH. Key-based login must work without a prompt.
      </p>
      <MachineCard
        v-for="m in state.machines"
        :key="m.id"
        :machine="m"
        @click="selectedId = m.id"
      />
      <p v-if="!state.machines.length" class="hint">No machines yet.</p>
    </section>
    <Dialog :open="adding" title="Add machine" @update:open="(o) => (adding = o)">
      <form class="add-form" @submit.prevent="add">
        <TextInput v-model="name" placeholder="Name" aria-label="Name" />
        <TextInput
          v-model="destination"
          placeholder="user@host or ~/.ssh/config alias"
          spellcheck="false"
          aria-label="SSH destination"
        />
        <p v-if="testResult" :class="testResult.ok ? 'muted' : 'error'">{{ testResult.text }}</p>
        <div class="settings-row">
          <Button type="button" :disabled="!destination.trim() || busy" @click="test"
            >Test connection</Button
          >
          <Button type="submit" variant="accent" :disabled="!canSubmit()">Add</Button>
        </div>
      </form>
    </Dialog>
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
header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
</style>
