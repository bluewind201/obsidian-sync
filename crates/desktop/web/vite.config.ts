import { defineConfig } from 'vite';
import path from 'path';

export default defineConfig({
  base: '/',
  build: {
    outDir: path.resolve(__dirname, '../web-dist'),
    emptyOutDir: true,
    sourcemap: false,
    chunkSizeWarningLimit: 3000,
  },
});
