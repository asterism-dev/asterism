<script setup lang="ts">
import { ask } from '@tauri-apps/plugin-dialog';
import { computed, ref, watch } from 'vue';
import { BASE_AGENTS } from '../settingsForm';
import { leaveSettings } from '../settingsGuard';
import { state } from '../store';
import { setTheme, themeChoice, type ThemeChoice } from '../theme';
import AgentSettings from './AgentSettings.vue';
import PathsSettings from './PathsSettings.vue';

const profiles = computed(() => ('hello' in state.node ? state.node.hello.agents : []));
const agents = computed(() => [...profiles.value.map((a) => a.name), ...BASE_AGENTS]);
const agent = ref(agents.value[0] ?? 'shell');
let picked = false;
const section = ref<'interface' | 'paths' | 'agents'>('interface');
const themes: { value: ThemeChoice; label: string }[] = [
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
];

// Opened before hello: switch to the first profile once it arrives, unless the user already chose.
watch(agents, (list) => {
  if (!picked && !state.settingsDirty && list[0]) agent.value = list[0];
});

function installed(name: string): boolean | null {
  const profile = profiles.value.find((a) => a.name === name);
  return profile ? profile.available : null;
}

async function discardChanges(): Promise<boolean> {
  return !state.settingsDirty || ask('Discard unsaved settings changes?', { title: 'Settings', kind: 'warning' });
}

async function pick(name: string) {
  if (name === agent.value || !(await discardChanges())) return;
  picked = true;
  agent.value = name;
}

async function open(next: 'interface' | 'paths' | 'agents') {
  if (next === section.value || !(await discardChanges())) return;
  state.settingsDirty = false;
  section.value = next;
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
        <button :class="{ active: section === 'interface' }" @click="open('interface')">Interface</button>
        <button :class="{ active: section === 'paths' }" @click="open('paths')">Paths</button>
        <button :class="{ active: section === 'agents' }" @click="open('agents')">Agents</button>
      </nav>
      <div v-if="section === 'interface'" class="settings-content">
        <section>
          <h3>Theme</h3>
          <div class="agent-picker" role="radiogroup" aria-label="Theme">
            <button
              v-for="t in themes"
              :key="t.value"
              role="radio"
              :aria-checked="themeChoice === t.value"
              :class="{ active: themeChoice === t.value }"
              @click="setTheme(t.value)"
            >
              {{ t.label }}
            </button>
          </div>
          <p class="muted">System follows your operating system's appearance. Terminals stay dark.</p>
        </section>
      </div>
      <div v-else-if="section === 'paths'" class="settings-content">
        <PathsSettings />
      </div>
      <div v-else class="settings-content">
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
.settings-content h3 { font-size: 13px; margin: 0 0 6px; }
</style>
