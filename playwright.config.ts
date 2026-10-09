import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests',
  testIgnore: '**/browser/**',
  use: { baseURL: 'http://127.0.0.1:1420', headless: true, viewport: { width: 1160, height: 900 } },
  webServer: { command: 'npm run dev', url: 'http://127.0.0.1:1420', reuseExistingServer: !process.env.CI },
});
