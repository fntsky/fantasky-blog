import vue from '@vitejs/plugin-vue'
import { defineConfig } from 'vite'

export default defineConfig({
  base: '/blog/',
  plugins: [vue()],
  server: {
    proxy: {
      '/blog/api': {
        target: 'http://127.0.0.1:53212',
        changeOrigin: true,
      },
    },
  },
})
