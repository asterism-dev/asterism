<script setup lang="ts">
import { ask } from '@tauri-apps/plugin-dialog';
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { api, errorMessage, RpcError } from '../../api';
import { CAPABILITY_FILTERS, hitAction, newPermissions, permissionText } from '../../pluginsView';
import { state, toast } from '../../store';
import type { CapabilityKind, PluginDetails, SearchHit } from '../../types';

const emit = defineEmits<{ configure: [name: string] }>();

const query = ref('');
const capability = ref<CapabilityKind | null>(null);
const store = ref<string | null>(null);
const stores = ref<string[]>([]);
const hits = ref<SearchHit[]>([]);
const error = ref<string | null>(null);
const selected = ref<SearchHit | null>(null);
const details = ref<PluginDetails | null>(null);
const detailsError = ref<string | null>(null);
const busy = ref(false);
let debounce: ReturnType<typeof setTimeout> | undefined;
let searchSeq = 0;

const key = (h: SearchHit) => `${h.store}/${h.name}`;
const action = computed(() => (selected.value ? hitAction(selected.value) : null));

async function search() {
  const seq = ++searchSeq;
  error.value = null;
  try {
    const found = await api.searchPlugins({
      query: query.value || undefined,
      capability: capability.value ?? undefined,
      store: store.value ?? undefined,
    });
    if (seq !== searchSeq) return;
    hits.value = found;
    if (selected.value) {
      const current = selected.value;
      const fresh = found.find((h) => key(h) === key(current));
      if (fresh) await select(fresh);
      else {
        selected.value = null;
        details.value = null;
      }
    }
  } catch (e) {
    if (seq !== searchSeq) return;
    hits.value = [];
    error.value = errorMessage(e);
  }
}

async function loadStores() {
  try {
    stores.value = (await api.storeList()).stores.map((s) => s.name);
    if (store.value && !stores.value.includes(store.value)) store.value = null;
  } catch {
    stores.value = [];
  }
}

async function select(hit: SearchHit) {
  selected.value = hit;
  details.value = null;
  detailsError.value = null;
  try {
    const loaded = await api.pluginDetails(hit.store, hit.name);
    if (selected.value === hit) details.value = loaded;
  } catch (e) {
    if (selected.value === hit) detailsError.value = errorMessage(e);
  }
}

async function install() {
  const hit = selected.value;
  const d = details.value;
  if (!hit || !d) return;
  busy.value = true;
  try {
    let question: string;
    let title: string;
    if (hit.installed_version) {
      const installed = (await api.plugins()).find((p) => p.name === d.name);
      const added = newPermissions(installed?.permissions ?? [], d.permissions).map(permissionText);
      question = `${d.name} ${d.version} asks for new permissions:\n\n${added.join('\n')}\n\nUpdate?`;
      title = 'Update plugin';
      if (!added.length) question = '';
    } else {
      const list = d.permissions.length
        ? d.permissions.map(permissionText).join('\n')
        : 'No special permissions';
      question = `Install ${d.name} ${d.version} from ${d.store}?\n\nIt asks for:\n${list}`;
      title = 'Install plugin';
    }
    if (question && !(await ask(question, { title, kind: 'warning' }))) return;
    const info = hit.installed_version
      ? await api.updatePlugin(d.name, d.permissions)
      : await api.installPlugin(d.store, d.name, d.permissions);
    toast(`${hit.installed_version ? 'Updated' : 'Installed'} ${d.name} ${info.version ?? ''}`);
    if (info.state.state === 'needs_setup') emit('configure', d.name);
  } catch (e) {
    if (e instanceof RpcError && e.kind === 'permissions_changed') {
      toast('The plugin changed its permissions; review them again.');
      await select(hit);
    } else {
      toast(errorMessage(e));
    }
  } finally {
    busy.value = false;
    await search();
  }
}

watch(query, () => {
  clearTimeout(debounce);
  debounce = setTimeout(search, 250);
});
watch([capability, store], search);
watch(
  () => [state.pluginsVersion, state.storesVersion],
  () => {
    void loadStores();
    void search();
  },
);
onBeforeUnmount(() => clearTimeout(debounce));
onMounted(() => {
  void loadStores();
  void search();
});
</script>

<template>
  <section class="discover">
    <div class="filters">
      <input
        v-model="query"
        placeholder="Search plugins"
        aria-label="Search plugins"
        spellcheck="false"
      />
      <select v-model="capability" aria-label="Capability">
        <option v-for="f in CAPABILITY_FILTERS" :key="f.label" :value="f.value">
          {{ f.label }}
        </option>
      </select>
      <select v-if="stores.length > 1" v-model="store" aria-label="Store">
        <option :value="null">All stores</option>
        <option v-for="s in stores" :key="s" :value="s">{{ s }}</option>
      </select>
    </div>
    <p v-if="error" class="error">{{ error }}</p>
    <p v-else-if="!hits.length" class="muted">No plugins found. Add a store under Stores.</p>
    <div class="split">
      <ul class="hit-list">
        <li v-for="h in hits" :key="key(h)">
          <button :class="{ active: selected && key(selected) === key(h) }" @click="select(h)">
            <strong>{{ h.name }}</strong>
            <span class="muted">{{ h.store }}</span>
            <span v-if="hitAction(h) !== 'install'" class="plugin-state">{{
              hitAction(h) === 'update' ? 'update' : 'installed'
            }}</span>
            <span v-if="h.description" class="muted description">{{ h.description }}</span>
          </button>
        </li>
      </ul>
      <article v-if="selected" class="details">
        <p v-if="detailsError" class="error">{{ detailsError }}</p>
        <p v-else-if="!details" class="muted">Loading…</p>
        <template v-else>
          <h3>
            {{ details.name }}
            <span class="muted">{{ details.version }} · {{ details.store }}</span>
          </h3>
          <p v-if="details.description">{{ details.description }}</p>
          <div class="chips">
            <span v-for="c in details.capabilities" :key="`${c.kind}:${c.id}`" class="chip"
              >{{ c.kind }}: {{ c.id }}</span
            >
          </div>
          <p class="muted">
            Permissions:
            {{
              details.permissions.length
                ? details.permissions.map(permissionText).join(', ')
                : 'none'
            }}
          </p>
          <p v-if="selected.linked" class="muted">
            A linked development copy overrides this plugin.
          </p>
          <button v-if="action !== 'installed'" :disabled="busy" @click="install">
            {{ busy ? 'Working…' : action === 'update' ? 'Update' : 'Install' }}
          </button>
          <pre v-if="details.readme" class="readme">{{ details.readme }}</pre>
        </template>
      </article>
    </div>
  </section>
</template>

<style scoped>
.filters {
  display: flex;
  gap: 8px;
  margin-bottom: 10px;
}
.filters input {
  flex: 1;
}
.split {
  display: grid;
  grid-template-columns: minmax(200px, 1fr) 2fr;
  gap: 12px;
}
.hit-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.hit-list button {
  width: 100%;
  text-align: left;
  border: 0;
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: baseline;
}
.hit-list button.active {
  background: var(--select);
}
.description {
  flex-basis: 100%;
}
.plugin-state {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 8px;
  background: var(--select);
}
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin: 6px 0;
}
.chip {
  font-size: 11px;
  padding: 1px 6px;
  border: 1px solid var(--border);
  border-radius: 8px;
}
.readme {
  white-space: pre-wrap;
  font-size: 12px;
  max-height: 50vh;
  overflow-y: auto;
  border-top: 1px solid var(--border);
  padding-top: 8px;
}
</style>
