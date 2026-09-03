import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],

  server: {
    proxy: {
      '/ingest': 'http://localhost:3001',
      '/upload': 'http://localhost:3001',
      '/v1/models': 'http://localhost:3001',
      '/v1/chat/completions': 'http://localhost:3001',
    },
  },
})
