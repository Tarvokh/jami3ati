import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import path from 'path';
import {defineConfig} from 'vite';

export default defineConfig(() => {
  // The UI calls `/api/v1/...` on its own origin; Vite forwards it to FastAPI.
  // That keeps the same code working from a phone on the LAN (the phone cannot
  // reach 127.0.0.1 of this machine, but it can reach this dev server).
  const apiTarget = process.env.VITE_API_PROXY_TARGET ?? 'http://127.0.0.1:8000';
  const proxy = {
    '/api': { target: apiTarget, changeOrigin: true },
  };

  return {
    plugins: [react(), tailwindcss()],
    resolve: {
      alias: {
        '@': path.resolve(__dirname, '.'),
      },
    },
    server: {
      // HMR is disabled in AI Studio via DISABLE_HMR env var.
      // Do not modify—file watching is disabled to prevent flickering during agent edits.
      hmr: process.env.DISABLE_HMR !== 'true',
      // Disable file watching when DISABLE_HMR is true to save CPU during agent edits.
      watch: process.env.DISABLE_HMR === 'true' ? null : {},
      proxy,
    },
    preview: { proxy },
  };
});
