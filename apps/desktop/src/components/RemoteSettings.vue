<script setup lang="ts">
import { Button, Dialog, NumberInput, Select, Switch, TextInput } from '@asterism-dev/lumen';
import { ask } from '@tauri-apps/plugin-dialog';
import QRCode from 'qrcode';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { api, errorMessage } from '../api';
import { relativeTime } from '../projects';
import {
  SERVE_OPTIONS,
  bindWarning,
  interfaceLabel,
  serveOffCommand,
  statusLine,
} from '../remoteSettings';
import { toast } from '../store';
import type {
  RemoteConfig,
  RemoteDevice,
  RemoteInterface,
  RemotePairing,
  RemoteStatus,
} from '../types';

const POLL_MS = 2000;
const status = ref<RemoteStatus | null>(null);
const interfaces = ref<RemoteInterface[]>([]);
const devices = ref<RemoteDevice[]>([]);
const form = ref<RemoteConfig>({
  enabled: false,
  bind: '127.0.0.1',
  port: 7420,
  serve: 'off',
  serve_port: 8443,
});
const newName = ref('');
const pairing = ref<{ name: string; url: string; qr: string } | null>(null);
const warning = computed(() => (form.value.serve === 'off' ? bindWarning(form.value.bind) : null));
const serving = computed(() => status.value?.config.serve ?? 'off');
const report = (e: unknown) => toast(errorMessage(e));
let timer: number | undefined;
let unmounted = false;
const CUSTOM = '__custom__';
const custom = ref(false);
const adding = ref(false);
const bindOptions = computed(() => [
  ...interfaces.value.map((i) => ({ value: i.address, label: interfaceLabel(i) })),
  { value: CUSTOM, label: 'Custom address…' },
]);
const selected = computed({
  get: () => (custom.value ? CUSTOM : form.value.bind),
  set: (v: string) => {
    custom.value = v === CUSTOM;
    if (!custom.value) form.value.bind = v;
  },
});

async function poll() {
  [status.value, devices.value] = await Promise.all([api.remoteStatus(), api.remoteDevices()]);
}

onMounted(async () => {
  try {
    interfaces.value = await api.remoteInterfaces();
    await poll();
    if (status.value) form.value = { ...status.value.config };
    if (!form.value.enabled && form.value.bind === '127.0.0.1') {
      const tailscale = interfaces.value.find((i) => i.tailscale);
      if (tailscale) form.value.bind = tailscale.address;
    }
    custom.value = !interfaces.value.some((i) => i.address === form.value.bind);
  } catch (e) {
    report(e);
  }
  if (unmounted) return;
  timer = window.setInterval(() => poll().catch(() => {}), POLL_MS);
});
onUnmounted(() => {
  unmounted = true;
  clearInterval(timer);
});

async function save() {
  try {
    await api.setRemoteConfig({ ...form.value, bind: form.value.bind.trim() });
    await poll();
  } catch (e) {
    report(e);
  }
}

function copyCommand(command: string) {
  navigator.clipboard.writeText(command).then(() => toast('Copied'), report);
}

async function showPairing(p: RemotePairing) {
  const qr = await QRCode.toDataURL(p.pairing_url).catch(() => '');
  pairing.value = { name: p.device.name, url: p.pairing_url, qr };
  await poll();
}

function addDevice() {
  const name = newName.value.trim();
  if (!name || adding.value) return;
  adding.value = true;
  api
    .addRemoteDevice(name)
    .then((p) => {
      newName.value = '';
      return showPairing(p);
    })
    .catch(report)
    .finally(() => (adding.value = false));
}

async function rotate(d: RemoteDevice) {
  const message = `Create a new token for ${d.name}? It has to be paired again.`;
  if (!(await ask(message, { title: 'New token', kind: 'warning' }))) return;
  api.rotateRemoteDevice(d.id).then(showPairing).catch(report);
}

async function remove(d: RemoteDevice) {
  const message = `Remove ${d.name}? It is disconnected right away.`;
  if (!(await ask(message, { title: 'Remove device', kind: 'warning' }))) return;
  api.removeRemoteDevice(d.id).then(poll).catch(report);
}

const now = () => Math.floor(Date.now() / 1000);

function lastSeen(d: RemoteDevice): string {
  if (d.connections) return 'connected';
  if (d.last_seen_at) return `last seen ${relativeTime(d.last_seen_at, now())}`;
  return 'never connected';
}
</script>

<template>
  <div class="agent-settings remote-settings">
    <section>
      <h3>Remote access</h3>
      <p class="hint">Use asterism from your phone or tablet over Tailscale.</p>
      <Switch v-model="form.enabled">Enable remote access</Switch>
      <div class="settings-row">
        <label for="remote-serve" class="serve-label">HTTPS via Tailscale serve</label>
        <Select id="remote-serve" v-model="form.serve" :options="SERVE_OPTIONS" />
        <NumberInput
          v-if="form.serve !== 'off'"
          :model-value="form.serve_port"
          min="1"
          max="65535"
          aria-label="HTTPS port"
          @update:model-value="(v) => v !== null && (form.serve_port = v)"
        />
      </div>
      <p v-if="form.serve !== 'off'" class="hint">
        Listens on 127.0.0.1 only; Tailscale serve forwards HTTPS to it.
      </p>
      <div class="settings-row">
        <Select
          v-if="form.serve === 'off'"
          v-model="selected"
          :options="bindOptions"
          aria-label="Interface"
        />
        <TextInput
          v-if="custom && form.serve === 'off'"
          v-model="form.bind"
          spellcheck="false"
          aria-label="Custom address"
        />
        <NumberInput
          :model-value="form.port"
          min="1"
          max="65535"
          aria-label="Port"
          @update:model-value="(v) => v !== null && (form.port = v)"
        />
        <Button variant="accent" @click="save">Save</Button>
      </div>
      <p v-if="warning" class="error">{{ warning }}</p>
      <p v-if="status" class="muted">{{ statusLine(status) }}</p>
      <template v-if="status && serving !== 'off'">
        <p v-if="status.public_url" class="muted">Phones open {{ status.public_url }}</p>
        <template v-if="serving === 'manual' && status.serve_command">
          <div class="settings-row">
            <TextInput
              class="command"
              :model-value="status.serve_command"
              readonly
              aria-label="Tailscale serve command"
              @focus="($event.target as HTMLInputElement).select()"
            />
            <Button @click="copyCommand(status.serve_command)">Copy</Button>
          </div>
          <p class="hint">
            Run it in a terminal. To stop it:
            <code>{{ serveOffCommand(status.serve_command) }}</code>
          </p>
        </template>
        <p class="hint">
          The first connection can take up to a minute while Tailscale issues the certificate.
        </p>
      </template>
    </section>
    <section>
      <h3>Devices</h3>
      <div v-for="d in devices" :key="d.id" class="settings-row device-row">
        <span>{{ d.name }}</span>
        <span class="hint">{{ lastSeen(d) }}</span>
        <Button @click="rotate(d)">New token</Button>
        <Button variant="danger" @click="remove(d)">Remove</Button>
      </div>
      <form class="settings-row" @submit.prevent="addDevice">
        <TextInput v-model="newName" placeholder="Device name, e.g. iPhone" />
        <Button type="submit" :disabled="!newName.trim() || adding">Add device</Button>
      </form>
    </section>
    <Dialog
      v-if="pairing"
      :open="true"
      :title="`Pair ${pairing.name}`"
      @update:open="(o) => !o && (pairing = null)"
    >
      <img
        v-if="pairing.qr"
        class="qr"
        :src="pairing.qr"
        alt="Pairing QR code"
        width="240"
        height="240"
      />
      <p class="hint">Scan with the device's camera. The code is shown only once.</p>
      <TextInput
        :model-value="pairing.url"
        readonly
        @focus="($event.target as HTMLInputElement).select()"
      />
      <template #actions><Button @click="pairing = null">Done</Button></template>
    </Dialog>
  </div>
</template>

<style scoped>
/* Settings stack with breathing room; the global .settings-row margin would double up. */
.remote-settings section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.remote-settings section > * {
  margin: 0;
}
.lm-choice {
  align-self: flex-start;
}
.serve-label {
  align-self: center;
  white-space: nowrap;
}
.device-row {
  align-items: center;
}
.device-row .hint {
  flex: 1;
}
.command {
  flex: 1;
  font-family: ui-monospace, Menlo, monospace;
}
.qr {
  align-self: center;
  background: #fff;
}
</style>
