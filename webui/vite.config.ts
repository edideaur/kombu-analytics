import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import fs from 'node:fs';
import path from 'node:path';

const umamiNodeModules = path.resolve(__dirname, '../umami/node_modules');
const webuiNodeModules = path.resolve(__dirname, 'node_modules');
if (!fs.existsSync(umamiNodeModules) && fs.existsSync(webuiNodeModules)) {
  try {
    fs.symlinkSync('../webui/node_modules', umamiNodeModules, 'junction');
  } catch (_e) {
  }
}

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, '../umami/src'),
      'next/navigation': path.resolve(__dirname, 'src/shims/next-navigation.ts'),
      'next/link': path.resolve(__dirname, 'src/shims/next-link.tsx'),
      'next/script': path.resolve(__dirname, 'src/shims/next-script.tsx'),
      'next-intl': path.resolve(__dirname, 'src/shims/next-intl.ts'),
    },
  },
  css: {
    postcss: path.resolve(__dirname, './postcss.config.js'),
  },
  publicDir: path.resolve(__dirname, '../umami/public'),
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks: {
          'vendor-react': ['react', 'react-dom', 'react-router'],
          'vendor-query': ['@tanstack/react-query'],
          'vendor-charts': ['chart.js', 'chartjs-adapter-date-fns'],
          'vendor-maps': ['react-simple-maps'],
          'vendor-rrweb': ['rrweb', 'rrweb-player'],
          'vendor-zen': ['@umami/react-zen'],
          'vendor-date': ['date-fns', 'date-fns-tz'],
          'vendor-icons': ['lucide-react'],
        },
      },
    },
  },
  server: {
    proxy: {
      '/api': 'http://localhost:3000',
    },
  },
});
