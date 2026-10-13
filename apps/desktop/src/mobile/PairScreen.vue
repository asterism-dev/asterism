<script setup lang="ts">
defineProps<{ revoked: boolean }>();
</script>

<template>
  <div class="pair">
    <h1>asterism</h1>
    <p v-if="revoked">This device is no longer paired.</p>
    <p>
      On your Mac, open Settings → Remote access, add this device and scan the QR code with this
      device's camera.
    </p>
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
