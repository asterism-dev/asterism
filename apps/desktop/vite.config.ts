/// <reference types="vitest/config" />
import vue from '@vitejs/plugin-vue';
import { defineConfig, loadEnv } from 'vite';

export default defineConfig(({ mode }) => ({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: Number(loadEnv(mode, '.', 'ASTERISM_').ASTERISM_DEV_PORT ?? 1420),
    strictPort: true,
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: { target: 'safari15', outDir: 'dist' },
  test: { environment: 'node' },
}));
