import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  base: '/dashboard/',
  plugins: [tailwindcss()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    assetsDir: 'assets',
    rollupOptions: {
      output: {
        entryFileNames: 'assets/dashboard.js',
        chunkFileNames: 'assets/[name].js',
        assetFileNames: (assetInfo) => {
          const names = [...(assetInfo.names ?? []), assetInfo.name ?? 'asset'];
          const name = names[0] ?? 'asset';
          if (names.some((candidate) => candidate.endsWith('styles.css') || candidate === 'index.css')) {
            return 'assets/dashboard.css';
          }
          if (name.endsWith('.css')) return 'assets/[name][extname]';
          return 'assets/[name][extname]';
        },
      },
    },
  },
});
