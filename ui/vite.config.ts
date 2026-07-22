import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [tailwindcss(), svelte()],
  // Tauri expects the dev server on a fixed port
  server: {
    port: 5173,
    strictPort: true,
  },
})
