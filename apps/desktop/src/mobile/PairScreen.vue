<script setup lang="ts">
import { ref } from 'vue';
import { connect } from './connection';
import { saveToken, tokenFromPairingInput } from './pairing';

defineProps<{ revoked: boolean }>();
const input = ref('');
const error = ref('');

function pairFromInput() {
  const token = tokenFromPairingInput(input.value);
  if (!token) {
    error.value = 'That is not a valid pairing link.';
    return;
  }
  error.value = '';
  saveToken(localStorage, token);
  connect(token);
}
</script>

<template>
  <div class="pair">
    <h1>asterism</h1>
    <p v-if="revoked">This device is no longer paired.</p>
    <p>
      On your Mac, open Settings → Remote access, add this device and scan the QR code with this
      device's camera.
    </p>
    <form @submit.prevent="pairFromInput">
      <input
        v-model="input"
        placeholder="Paste pairing link"
        aria-label="Pairing link"
        autocapitalize="off"
        autocorrect="off"
        spellcheck="false"
      />
      <button type="submit">Pair</button>
      <p v-if="error" role="alert">{{ error }}</p>
    </form>
  </div>
</template>

<style scoped>
.pair {
  max-width: 28rem;
  margin: 20vh auto 0;
  padding: 0 16px;
  text-align: center;
}
</style>
