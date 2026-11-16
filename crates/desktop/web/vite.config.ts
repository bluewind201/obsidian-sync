import { defineConfig } from 'vite';
import path from 'path';

export default defineConfig({
  base: '/',
  plugins: [
    {
      name: 'strip-crossorigin-for-embedded-webview',
      transformIndexHtml(html) {
        return html.replace(/\s+crossorigin(?=[\s/>])/g, '');
      },
    },
  ],
  build: {
    outDir: path.resolve(__dirname, '../web-dist'),
    emptyOutDir: true,
    sourcemap: false,
    chunkSizeWarningLimit: 3000,
  },
});
