<script setup lang="ts">
import { Button, Checkbox, TextInput, Textarea } from '@asterism-dev/lumen';
import { openUrl } from '@tauri-apps/plugin-opener';
import { X } from '@lucide/vue';
import { computed, ref, watch } from 'vue';
import { api, errorMessage, RpcError } from '../api';
import {
  agentSections,
  BASE_AGENTS,
  commandPreview,
  copyLocalEnv,
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

// Embedded forms leave saving and the dirty guard to the parent's shared save bar.
const props = defineProps<{ agent: string; info?: AgentInfo; embedded?: boolean }>();

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

watch(
  dirty,
  (value) => {
    if (!props.embedded) state.settingsDirty = value;
  },
  { immediate: true },
);

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
        ...toForm({
          args: raw.args,
          env: raw.env,
          machine_env: raw.machine_env,
          machine_env_from_local: raw.machine_env_from_local,
          mcp: null,
          hooks: null,
          hibernate_after_min: raw.hibernate_after_min,
        }),
        mcpText: raw.mcp_text ?? '',
        hooksText: raw.hooks_text ?? '',
      };
    } catch {
      // Keep the empty form; loadError already explains the problem.
    }
  }
  saved.value = JSON.stringify(form.value);
}

async function save(): Promise<boolean> {
  const result = fromForm(form.value, sections.value);
  if ('errors' in result) {
    errors.value = result.errors;
    return false;
  }
  saving.value = true;
  try {
    await api.setAgentConfig(props.agent, result.config);
    saved.value = JSON.stringify(form.value);
    errors.value = {};
    loadError.value = null;
    if (!props.embedded) toast(`Saved ${props.agent} settings`);
    return true;
  } catch (e) {
    errors.value = { save: errorMessage(e) };
    return false;
  } finally {
    saving.value = false;
  }
}

defineExpose({ dirty, save });

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
        <TextInput v-model="form.args[i]" placeholder="--model" spellcheck="false" />
        <Button
          variant="ghost"
          :icon="X"
          title="Remove"
          aria-label="Remove"
          @click="form.args.splice(i, 1)"
        />
      </div>
      <Button @click="form.args.push('')">+ Parameter</Button>
      <p class="muted mono">{{ commandPreview(agent, form) }}</p>
    </section>

    <section>
      <h3>Environment</h3>
      <p class="muted">
        CLAUDE* and ANTHROPIC_* are never inherited — set them here if sessions need them.
      </p>
      <div v-for="(row, i) in form.set" :key="i" class="settings-row">
        <TextInput v-model="row.key" placeholder="NAME" spellcheck="false" />
        <TextInput v-model="row.value" placeholder="value" spellcheck="false" />
        <Button
          variant="ghost"
          :icon="X"
          title="Remove"
          aria-label="Remove"
          @click="form.set.splice(i, 1)"
        />
      </div>
      <Button @click="form.set.push({ key: '', value: '' })">+ Variable</Button>
      <h4>Remove inherited</h4>
      <div v-for="(_, i) in form.remove" :key="i" class="settings-row">
        <TextInput v-model="form.remove[i]" placeholder="AWS_*" spellcheck="false" />
        <Button
          variant="ghost"
          :icon="X"
          title="Remove"
          aria-label="Remove"
          @click="form.remove.splice(i, 1)"
        />
      </div>
      <Button @click="form.remove.push('')">+ Pattern</Button>
      <p v-if="errors.env" class="error">{{ errors.env }}</p>
    </section>

    <section v-if="!BASE_AGENTS.includes(agent)">
      <h3>Environment on machines</h3>
      <p class="muted">
        Sent to machines instead of the environment above, which stays on this computer unless "Same
        as local" is on.
      </p>
      <Checkbox v-model="form.machineFromLocal">Same as local</Checkbox>
      <template v-if="!form.machineFromLocal">
        <Button @click="copyLocalEnv(form)">Copy from local</Button>
        <div v-for="(row, i) in form.machineSet" :key="i" class="settings-row">
          <TextInput v-model="row.key" placeholder="NAME" spellcheck="false" />
          <TextInput v-model="row.value" placeholder="value" spellcheck="false" />
          <Button
            variant="ghost"
            :icon="X"
            title="Remove"
            aria-label="Remove"
            @click="form.machineSet.splice(i, 1)"
          />
        </div>
        <Button @click="form.machineSet.push({ key: '', value: '' })">+ Variable</Button>
        <h4>Remove inherited</h4>
        <div v-for="(_, i) in form.machineRemove" :key="i" class="settings-row">
          <TextInput v-model="form.machineRemove[i]" placeholder="AWS_*" spellcheck="false" />
          <Button
            variant="ghost"
            :icon="X"
            title="Remove"
            aria-label="Remove"
            @click="form.machineRemove.splice(i, 1)"
          />
        </div>
        <Button @click="form.machineRemove.push('')">+ Pattern</Button>
      </template>
      <p v-if="errors.machineEnv" class="error">{{ errors.machineEnv }}</p>
    </section>

    <section v-if="sections.mcp">
      <h3>MCP servers</h3>
      <p class="muted">
        {{ label }}'s MCP format, added to your own {{ label }} configuration.
        <a href="#" @click.prevent="docs(MCP_DOCS)">Docs</a>
      </p>
      <Textarea
        v-model="form.mcpText"
        class="json"
        :rows="8"
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
      <Textarea
        v-model="form.hooksText"
        class="json"
        :rows="8"
        :placeholder="HOOKS_EXAMPLE"
        spellcheck="false"
      />
      <p v-if="errors.hooks" class="error">{{ errors.hooks }}</p>
    </section>

    <section v-if="sections.hibernate">
      <h3>Hibernation</h3>
      <p class="muted">
        Ends idle {{ label }} sessions that are not open in a pane and resumes them when opened.
      </p>
      <label>
        Hibernate after (minutes, 0 = off)
        <TextInput v-model="form.hibernateAfter" inputmode="numeric" placeholder="30" />
      </label>
      <p v-if="errors.hibernate" class="error">{{ errors.hibernate }}</p>
    </section>

    <div v-if="!embedded" class="save-bar">
      <Button
        :variant="dirty ? 'accent' : 'default'"
        :disabled="saving || !dirty || gone"
        @click="save"
      >
        Save
      </Button>
      <span v-if="gone" class="muted">This agent is no longer installed.</span>
      <span v-else class="muted">Applies to newly started sessions.</span>
    </div>
    <p v-if="errors.save" class="error">{{ errors.save }}</p>
  </div>
</template>
