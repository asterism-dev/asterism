<script setup lang="ts">
import { ask } from '@tauri-apps/plugin-dialog';
import { computed, ref } from 'vue';
import { BASE_AGENTS } from '../settingsForm';
import { leaveSettings } from '../settingsGuard';
import { state } from '../store';
import AgentSettings from './AgentSettings.vue';

const profiles = computed(() => ('hello' in state.node ? state.node.hello.agents : []));
const agents = computed(() => [...profiles.value.map((a) => a.name), ...BASE_AGENTS]);
const agent = ref(agents.value[0] ?? 'shell');

function installed(name: string): boolean | null {
  const profile = profiles.value.find((a) => a.name === name);
  return profile ? profile.available : null;
}

async function pick(name: string) {
  if (name === agent.value) return;
  if (state.settingsDirty && !(await ask('Discard unsaved settings changes?', { title: 'Settings', kind: 'warning' }))) {
    return;
  }
  agent.value = name;
}
</script>

<template>
  <section class="settings-view">
    <header class="settings-header">
      <button @click="leaveSettings()">← Back</button>
      <h1>Settings</h1>
    </header>
    <div class="settings-body">
      <nav class="settings-nav">
        <button class="active">Agents</button>
      </nav>
      <div class="settings-content">
        <div class="agent-picker">
          <button v-for="name in agents" :key="name" :class="{ active: name === agent }" @click="pick(name)">
            {{ name }}
            <span v-if="installed(name) === false" class="muted">(not installed)</span>
          </button>
        </div>
        <AgentSettings :key="agent" :agent="agent" />
      </div>
    </div>
  </section>
</template>

<style scoped>
.settings-view { display: flex; flex-direction: column; flex: 1; min-height: 0; }
.settings-header { display: flex; align-items: center; gap: 12px; padding: 10px 14px; border-bottom: 1px solid var(--border); background: var(--panel); }
.settings-header h1 { font-size: 15px; margin: 0; }
.settings-body { display: grid; grid-template-columns: 160px 1fr; flex: 1; min-height: 0; }
.settings-nav { border-right: 1px solid var(--border); padding: 10px 8px; display: flex; flex-direction: column; gap: 4px; }
.settings-nav button, .agent-picker button { border: 0; text-align: left; }
.settings-nav button.active, .agent-picker button.active { background: var(--select); }
.settings-content { overflow-y: auto; padding: 14px 18px; }
.agent-picker { display: flex; gap: 6px; margin-bottom: 12px; }
</style>
