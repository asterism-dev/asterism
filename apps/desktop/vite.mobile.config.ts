/// <reference types="vitest/config" />
import vue from '@vitejs/plugin-vue';
import { defineConfig, loadEnv } from 'vite';

const shim = (name: string) =>
  decodeURIComponent(new URL(`./src/mobile/shims/${name}`, import.meta.url).pathname);

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, '.', 'ASTERISM_');
  return {
    plugins: [vue()],
    clearScreen: false,
    resolve: {
      alias: [
        { find: '@tauri-apps/api/core', replacement: shim('core.ts') },
        { find: '@tauri-apps/api/window', replacement: shim('window.ts') },
        { find: '@tauri-apps/plugin-dialog', replacement: shim('dialog.ts') },
        { find: '@tauri-apps/plugin-opener', replacement: shim('opener.ts') },
        { find: /^.*\/dock\/main$/, replacement: shim('dock.ts') },
      ],
    },
    server: {
      host: true,
      port: Number(env.ASTERISM_MOBILE_DEV_PORT ?? 1421),
      strictPort: true,
      // The daemon's remote listener; proxied so the dev page stays same-origin.
      proxy: {
        '/ws': { target: `ws://127.0.0.1:${env.ASTERISM_REMOTE_PORT ?? 7420}`, ws: true },
      },
    },
    build: {
      target: 'safari15',
      outDir: 'dist-mobile',
      emptyOutDir: true,
      rollupOptions: { input: 'mobile.html' },
    },
  };
});
