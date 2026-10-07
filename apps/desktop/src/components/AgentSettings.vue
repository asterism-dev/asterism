<script setup lang="ts">
import { openUrl } from '@tauri-apps/plugin-opener';
import { X } from '@lucide/vue';
import { computed, ref, watch } from 'vue';
import { api, errorMessage, RpcError } from '../api';
import {
  agentSections,
  BASE_AGENTS,
  commandPreview,
  emptyForm,
  fromForm,
  HOOKS_EXAMPLE,
  MCP_EXAMPLE,
  toForm,
  type AgentForm,
  type FormErrors,
} from '../settingsForm';
import { state, toast } from '../store';
import type { AgentInfo } from '../types';

const props = defineProps<{ agent: string; info?: AgentInfo }>();

const MCP_DOCS = 'https://docs.anthropic.com/en/docs/claude-code/mcp';
const HOOKS_DOCS = 'https://docs.anthropic.com/en/docs/claude-code/hooks';

const form = ref<AgentForm>(emptyForm());
const saved = ref(JSON.stringify(emptyForm()));
const errors = ref<FormErrors>({});
const loadError = ref<string | null>(null);
const saving = ref(false);
const sections = computed(() => agentSections(props.info));
const label = computed(() => props.info?.display_name || props.agent);
const gone = computed(() => !props.info && !BASE_AGENTS.includes(props.agent));
const dirty = computed(() => JSON.stringify(form.value) !== saved.value);

watch(dirty, (value) => (state.settingsDirty = value), { immediate: true });

async function load() {
  errors.value = {};
  loadError.value = null;
  try {
    form.value = toForm(await api.agentConfig(props.agent));
  } catch (e) {
    if (e instanceof RpcError && e.kind === 'method_not_found') {
      loadError.value =
        'The running daemon is too old for settings. Restart it from the node menu (right-click the computer name).';
      return;
    }
    // Broken hand-edited files: show their raw text so they can be repaired here.
    loadError.value = errorMessage(e);
    form.value = emptyForm();
    try {
      const raw = await api.agentConfigRaw(props.agent);
      form.value = {
        ...toForm({ args: raw.args, env: raw.env, mcp: null, hooks: null }),
        mcpText: raw.mcp_text ?? '',
        hooksText: raw.hooks_text ?? '',
      };
    } catch {
      // Keep the empty form; loadError already explains the problem.
    }
  }
  saved.value = JSON.stringify(form.value);
}

async function save() {
  const result = fromForm(form.value, sections.value);
  if ('errors' in result) {
    errors.value = result.errors;
    return;
  }
  saving.value = true;
  try {
    await api.setAgentConfig(props.agent, result.config);
    saved.value = JSON.stringify(form.value);
    errors.value = {};
    loadError.value = null;
    toast(`Saved ${props.agent} settings`);
  } catch (e) {
    errors.value = { save: errorMessage(e) };
  } finally {
    saving.value = false;
  }
}

function docs(url: string) {
  openUrl(url).catch((e) => toast(errorMessage(e)));
}

watch(() => props.agent, load, { immediate: true });
</script>

<template>
  <div class="agent-settings">
    <p v-if="loadError" class="error">{{ loadError }}</p>

    <section v-if="sections.args">
      <h3>Parameters</h3>
      <div v-for="(_, i) in form.args" :key="i" class="settings-row">
        <input v-model="form.args[i]" placeholder="--model" spellcheck="false" />
        <button title="Remove" aria-label="Remove" @click="form.args.splice(i, 1)"><X /></button>
      </div>
      <button @click="form.args.push('')">+ Parameter</button>
      <p class="muted mono">{{ commandPreview(agent, form) }}</p>
    </section>

    <section>
      <h3>Environment</h3>
      <p class="muted">
        CLAUDE* and ANTHROPIC_* are never inherited — set them here if sessions need them.
      </p>
      <div v-for="(row, i) in form.set" :key="i" class="settings-row">
        <input v-model="row.key" placeholder="NAME" spellcheck="false" />
        <input v-model="row.value" placeholder="value" spellcheck="false" />
        <button title="Remove" aria-label="Remove" @click="form.set.splice(i, 1)"><X /></button>
      </div>
      <button @click="form.set.push({ key: '', value: '' })">+ Variable</button>
      <h4>Remove inherited</h4>
      <div v-for="(_, i) in form.remove" :key="i" class="settings-row">
        <input v-model="form.remove[i]" placeholder="AWS_*" spellcheck="false" />
        <button title="Remove" aria-label="Remove" @click="form.remove.splice(i, 1)"><X /></button>
      </div>
      <button @click="form.remove.push('')">+ Pattern</button>
      <p v-if="errors.env" class="error">{{ errors.env }}</p>
    </section>

    <section v-if="sections.mcp">
      <h3>MCP servers</h3>
      <p class="muted">
        {{ label }}'s MCP format, added to your own {{ label }} configuration.
        <a href="#" @click.prevent="docs(MCP_DOCS)">Docs</a>
      </p>
      <textarea
        v-model="form.mcpText"
        class="json"
        rows="8"
        :placeholder="MCP_EXAMPLE"
        spellcheck="false"
      />
      <p v-if="errors.mcp" class="error">{{ errors.mcp }}</p>
    </section>

    <section v-if="sections.hooks">
      <h3>Hooks</h3>
      <p class="muted">
        {{ label }}'s hooks format. asterism's status hooks stay active as well.
        <a href="#" @click.prevent="docs(HOOKS_DOCS)">Docs</a>
      </p>
      <textarea
        v-model="form.hooksText"
        class="json"
        rows="8"
        :placeholder="HOOKS_EXAMPLE"
        spellcheck="false"
      />
      <p v-if="errors.hooks" class="error">{{ errors.hooks }}</p>
    </section>

    <div class="save-bar">
      <button :class="{ primary: dirty }" :disabled="saving || !dirty || gone" @click="save">
        Save
      </button>
      <span v-if="gone" class="muted">This agent is no longer installed.</span>
      <span v-else class="muted">Applies to newly started sessions.</span>
    </div>
    <p v-if="errors.save" class="error">{{ errors.save }}</p>
  </div>
</template>
