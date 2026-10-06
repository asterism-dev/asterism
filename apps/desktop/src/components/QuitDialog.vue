<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { api, errorMessage } from '../api';
import { toast } from '../store';

defineProps<{ running: number }>();
const emit = defineEmits<{ close: [] }>();

const busy = ref(false);
const keepButton = ref<HTMLButtonElement>();

onMounted(() => keepButton.value?.focus());

function close() {
  if (!busy.value) emit('close');
}

async function quit(stopDaemon: boolean) {
  busy.value = true;
  try {
    await api.quit(stopDaemon);
  } catch (e) {
    toast(errorMessage(e));
    busy.value = false;
  }
}
</script>

<template>
  <div class="modal-backdrop" tabindex="-1" @click.self="close" @keydown.esc="close">
    <div class="modal" role="dialog" aria-modal="true" aria-label="Quit asterism">
      <strong>Stop the daemon too?</strong>
      <p class="muted">
        {{ running === 1 ? '1 session is' : `${running} sessions are` }} still running. Keep the daemon running to continue them
        later, or stop it and end them.
      </p>
      <div class="actions">
        <button type="button" :disabled="busy" @click="close">Cancel</button>
        <button type="button" :disabled="busy" @click="quit(true)">Stop daemon</button>
        <button ref="keepButton" type="button" class="primary" :disabled="busy" @click="quit(false)">Keep running</button>
      </div>
    </div>
  </div>
</template>
