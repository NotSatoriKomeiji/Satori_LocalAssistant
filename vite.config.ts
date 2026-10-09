import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
export default defineConfig({
  plugins: [svelte()],
  server: { port: 1420, strictPort: true, host: '127.0.0.1' },
  clearScreen: false,
});
