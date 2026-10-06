<script setup lang="ts">
import { ask } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { computed, ref, watch } from 'vue';
import { api } from '../api';
import { pluginTabs, type PluginTab } from '../pluginsView';
import { BASE_AGENTS } from '../settingsForm';
import { leaveSettings } from '../settingsGuard';
import { state } from '../store';
import { setTheme, themeChoice, type ThemeChoice } from '../theme';
import { checkForUpdates, installUpdate, RELEASES_URL, updateLabel, updater } from '../updater';
import AgentSettings from './AgentSettings.vue';
import PathsSettings from './PathsSettings.vue';
import PluginSettingsForm from './plugins/PluginSettingsForm.vue';
import PluginsSettings from './PluginsSettings.vue';

type Section = 'interface' | 'paths' | 'sessions' | 'plugins' | 'about' | `plugin:${string}`;
const section = ref<Section>('interface');
const sessionKind = ref(BASE_AGENTS[0]);
const tabs = ref<PluginTab[]>([]);
const plugin = computed(() => (section.value.startsWith('plugin:') ? section.value.slice('plugin:'.length) : null));
const pluginAgents = computed(() => state.agents.filter((a) => a.plugin === plugin.value));

watch(() => state.pluginSettingsRequest, (name) => {
  if (!name) return;
  section.value = `plugin:${name}`;
  state.pluginSettingsRequest = null;
}, { immediate: true });

async function loadTabs() {
  try {
    const plugins = await api.plugins();
    const withSettings: string[] = [];
    await Promise.all(plugins.map(async (p) => {
      // Broken or disabled plugins may not report settings; they get no tab anyway.
      const settings = await api.pluginSettings(p.name).catch(() => null);
      if (settings?.schema.length) withSettings.push(p.name);
    }));
    tabs.value = pluginTabs(plugins, withSettings, state.agents);
  } catch {
    // Keep the previous tabs; the Plugins section shows the error.
  }
}
watch(() => [state.pluginsVersion, state.agents], loadTabs, { immediate: true });
const themes: { value: ThemeChoice; label: string }[] = [
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
];

async function discardChanges(): Promise<boolean> {
  return !state.settingsDirty || ask('Discard unsaved settings changes?', { title: 'Settings', kind: 'warning' });
}

async function pick(name: string) {
  if (name === sessionKind.value || !(await discardChanges())) return;
  sessionKind.value = name;
}

async function open(next: Section) {
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
        <button :class="{ active: section === 'sessions' }" @click="open('sessions')">Sessions</button>
        <button :class="{ active: section === 'plugins' }" @click="open('plugins')">Plugins</button>
        <button v-for="t in tabs" :key="t.name" :class="{ active: plugin === t.name }" @click="open(`plugin:${t.name}`)">{{ t.title }}</button>
        <button :class="{ active: section === 'about' }" @click="open('about')">About</button>
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
      <div v-else-if="section === 'plugins'" class="settings-content">
        <PluginsSettings />
      </div>
      <div v-else-if="plugin" class="settings-content">
        <PluginSettingsForm :key="plugin" :plugin="plugin" />
        <template v-for="a in pluginAgents" :key="`${plugin}:${a.name}`">
          <h2 v-if="pluginAgents.length > 1">{{ a.display_name }}</h2>
          <p v-if="!a.available" class="muted">{{ a.display_name }} is not installed on this node.</p>
          <AgentSettings :agent="a.name" :info="a" />
        </template>
      </div>
      <div v-else-if="section === 'about'" class="settings-content">
        <section>
          <h3>asterism {{ updater.current }}</h3>
          <div class="about-actions">
            <button :disabled="updater.checking || updater.installing" @click="checkForUpdates(true)">
              {{ updater.checking ? 'Checking…' : 'Check for updates' }}
            </button>
            <button v-if="updater.available" :disabled="updater.installing" @click="installUpdate()">{{ updateLabel() }}</button>
          </div>
          <template v-if="updater.available">
            <p class="muted">Version {{ updater.available.version }} is available.</p>
            <pre v-if="updater.available.notes" class="release-notes">{{ updater.available.notes }}</pre>
          </template>
          <p v-else-if="updater.checked" class="muted">You're up to date.</p>
          <p><a href="#" @click.prevent="openUrl(RELEASES_URL)">All releases</a></p>
        </section>
      </div>
      <div v-else class="settings-content">
        <div class="agent-picker">
          <button v-for="name in BASE_AGENTS" :key="name" :class="{ active: name === sessionKind }" @click="pick(name)">
            {{ name.charAt(0).toUpperCase() + name.slice(1) }}
          </button>
        </div>
        <AgentSettings :key="sessionKind" :agent="sessionKind" />
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
.about-actions { display: flex; gap: 8px; margin: 8px 0; }
.release-notes { white-space: pre-wrap; font-size: 12px; max-height: 240px; overflow-y: auto; }
</style>
