<script setup lang="ts">
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
  <div class="agent-settings">
    <section>
      <h3>Remote access</h3>
      <p class="hint">Use asterism from your phone or tablet over Tailscale.</p>
      <label
        ><input v-model="form.enabled" type="checkbox" role="switch" /> Enable remote access</label
      >
      <div class="settings-row">
        <label for="remote-serve">HTTPS via Tailscale serve</label>
        <select id="remote-serve" v-model="form.serve">
          <option v-for="o in SERVE_OPTIONS" :key="o.value" :value="o.value">{{ o.label }}</option>
        </select>
        <input
          v-if="form.serve !== 'off'"
          v-model.number="form.serve_port"
          type="number"
          min="1"
          max="65535"
          aria-label="HTTPS port"
        />
      </div>
      <p v-if="form.serve !== 'off'" class="hint">
        Listens on 127.0.0.1 only; Tailscale serve forwards HTTPS to it.
      </p>
      <div class="settings-row">
        <select v-if="form.serve === 'off'" v-model="selected" aria-label="Interface">
          <option v-for="i in interfaces" :key="i.address" :value="i.address">
            {{ interfaceLabel(i) }}
          </option>
          <option :value="CUSTOM">Custom address…</option>
        </select>
        <input
          v-if="custom && form.serve === 'off'"
          v-model="form.bind"
          spellcheck="false"
          aria-label="Custom address"
        />
        <input v-model.number="form.port" type="number" min="1" max="65535" aria-label="Port" />
        <button @click="save">Save</button>
      </div>
      <p v-if="warning" class="error">{{ warning }}</p>
      <p v-if="status" class="muted">{{ statusLine(status) }}</p>
      <template v-if="status && serving !== 'off'">
        <p v-if="status.public_url" class="muted">Phones open {{ status.public_url }}</p>
        <template v-if="serving === 'manual' && status.serve_command">
          <div class="settings-row">
            <input
              class="command"
              :value="status.serve_command"
              readonly
              aria-label="Tailscale serve command"
              @focus="($event.target as HTMLInputElement).select()"
            />
            <button @click="copyCommand(status.serve_command)">Copy</button>
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
        <button @click="rotate(d)">New token</button>
        <button class="danger" @click="remove(d)">Remove</button>
      </div>
      <form class="settings-row" @submit.prevent="addDevice">
        <input v-model="newName" placeholder="Device name, e.g. iPhone" />
        <button type="submit" :disabled="!newName.trim() || adding">Add device</button>
      </form>
    </section>
    <div v-if="pairing" class="modal-backdrop" @click.self="pairing = null">
      <div class="modal">
        <h3>Pair {{ pairing.name }}</h3>
        <img v-if="pairing.qr" :src="pairing.qr" alt="Pairing QR code" width="240" height="240" />
        <p class="hint">Scan with the device's camera. The code is shown only once.</p>
        <input
          :value="pairing.url"
          readonly
          @focus="($event.target as HTMLInputElement).select()"
        />
        <div class="actions"><button @click="pairing = null">Done</button></div>
      </div>
    </div>
  </div>
</template>

<style scoped>
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
.danger {
  color: var(--danger);
}
.modal img {
  align-self: center;
  background: #fff;
}
</style>
