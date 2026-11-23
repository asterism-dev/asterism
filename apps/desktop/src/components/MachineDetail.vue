<script setup lang="ts">
import { Button, Switch } from '@asterism-dev/lumen';
import { ChevronRight, Ellipsis } from '@lucide/vue';
import { computed, onMounted, ref } from 'vue';
import { api, errorMessage } from '../api';
import { canUpdateMachine, machineStatusLine, machineTaskList, providerLabel } from '../machines';
import { leaveSettings } from '../settingsGuard';
import { showMenu, state, taskStatus, toast } from '../store';
import type { Machine, PluginInfo, Task } from '../types';
import MachineDot from './MachineDot.vue';

const props = defineProps<{ machine: Machine; updating: boolean }>();
const emit = defineEmits<{ back: []; update: []; remove: []; forget: [] }>();

const tasks = computed(() => machineTaskList(props.machine, state.tasks));
const status = computed(() => props.machine.status);
const needsRetry = computed(
  () => status.value.state !== 'ready' && status.value.state !== 'connecting',
);
const plugins = ref<PluginInfo[]>([]);
onMounted(() => {
  api.plugins().then(
    (l) => (plugins.value = l),
    () => {},
  );
});
const agents = computed(() => status.value.agents ?? []);

const projectName = (t: Task) => state.projects.find((p) => p.id === t.project_id)?.name;
const taskLine = (t: Task) => [projectName(t), taskStatus(state, t.id)].filter(Boolean).join(' · ');

function openTask(t: Task) {
  leaveSettings()
    .then((left) => {
      if (!left) return;
      state.projectPage = null;
      state.selectedTaskId = t.id;
    })
    .catch((e) => toast(errorMessage(e)));
}

function menu(e: MouseEvent) {
  showMenu(e, [
    { label: 'Remove', danger: true, action: () => emit('remove') },
    { label: 'Forget', danger: true, action: () => emit('forget') },
  ]);
}
</script>

<template>
  <div class="detail">
    <header>
      <h3>
        <button type="button" class="crumb" @click="emit('back')">Machines</button>
        <ChevronRight :size="12" /> {{ machine.name }}
      </h3>
      <Button
        variant="ghost"
        :icon="Ellipsis"
        title="Machine actions"
        aria-label="Machine actions"
        @click="menu"
      />
    </header>
    <section class="card">
      <h4>Connection</h4>
      <div class="line">
        <MachineDot :machine="machine" />
        <span class="grow">
          {{ machineStatusLine(machine) }} · <span class="muted">{{ machine.destination }}</span>
        </span>
        <Button
          v-if="needsRetry"
          @click="api.retryMachine(machine.id).catch((e) => toast(errorMessage(e)))"
          >Retry</Button
        >
      </div>
      <p v-if="status.error" class="error">{{ status.error }}</p>
      <div v-if="machine.provider" class="line">
        <span class="grow muted"
          >Provider: {{ providerLabel(plugins, machine.provider.provider) }}</span
        >
        <template v-if="machine.provider.owned">
          <label for="delete-with-tasks">Delete VM with its last task</label>
          <Switch
            id="delete-with-tasks"
            :model-value="machine.provider.delete_with_tasks"
            @update:model-value="
              api.setMachineDeleteWithTasks(machine.id, $event).catch((e) => toast(errorMessage(e)))
            "
          />
        </template>
      </div>
    </section>
    <section class="card">
      <h4>Asterism on the machine</h4>
      <div class="line">
        <template v-if="canUpdateMachine(machine)">
          <span class="grow">Update available</span>
          <Button :disabled="updating" @click="emit('update')">{{
            updating ? 'Updating…' : 'Update now'
          }}</Button>
        </template>
        <span v-else-if="status.state === 'ready'">✓ Up to date</span>
        <span v-else class="muted">Known once connected</span>
      </div>
    </section>
    <section class="card">
      <h4>Agents</h4>
      <div v-if="agents.length" class="line agents">
        <span v-for="a in agents" :key="a.name">
          {{ a.display_name || a.name }} <template v-if="a.available">✓</template>
          <span v-else class="muted">– not installed</span>
        </span>
      </div>
      <span v-else class="muted">Shown once connected</span>
    </section>
    <section class="card">
      <h4>Tasks on this machine</h4>
      <button v-for="t in tasks" :key="t.id" type="button" class="task" @click="openTask(t)">
        <span class="grow"
          >{{ t.title }} <span class="muted">{{ taskLine(t) }}</span></span
        >
        <ChevronRight :size="14" />
      </button>
      <span v-if="!tasks.length" class="muted">No active tasks</span>
    </section>
  </div>
</template>

<style scoped>
.detail {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
h3 {
  display: flex;
  align-items: center;
  gap: 4px;
  margin: 0;
}
.crumb {
  background: none;
  border: none;
  padding: 0;
  font: inherit;
  color: var(--muted);
  cursor: pointer;
}
.crumb:hover {
  color: var(--text);
}
.card {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 10px 12px;
  margin: 0;
}
.card h4 {
  margin: 0 0 6px;
}
.card p {
  margin: 6px 0 0;
}
.line {
  display: flex;
  align-items: center;
  gap: 8px;
}
.agents {
  gap: 16px;
}
.grow {
  flex: 1;
}
.task {
  display: flex;
  align-items: center;
  width: 100%;
  padding: 4px 0;
  background: none;
  border: none;
  font: inherit;
  color: inherit;
  text-align: left;
  cursor: pointer;
}
.task:hover {
  background: var(--select);
}
</style>
