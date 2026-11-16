<script setup lang="ts">
import { Button, Dialog } from '@asterism-dev/lumen';
import { onMounted, ref } from 'vue';
import { api, errorMessage } from '../api';
import { toast } from '../store';

defineProps<{ running: number }>();
const emit = defineEmits<{ close: [] }>();

const busy = ref(false);
const keepButton = ref<InstanceType<typeof Button>>();

onMounted(() => keepButton.value?.$el.focus());

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
  <Dialog :open="true" title="Stop the daemon too?" @update:open="(o) => !o && close()">
    <p class="muted">
      {{ running === 1 ? '1 session is' : `${running} sessions are` }} still running. Keep the
      daemon running to continue them later, or stop it and end them.
    </p>
    <template #actions>
      <Button :disabled="busy" @click="close">Cancel</Button>
      <Button :disabled="busy" @click="quit(true)">Stop daemon</Button>
      <Button ref="keepButton" variant="accent" :disabled="busy" @click="quit(false)">
        Keep running
      </Button>
    </template>
  </Dialog>
</template>
