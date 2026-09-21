import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'node:path';

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
  test: {
    environment: 'jsdom',
    globals: true,
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/shims/**/*.{ts,tsx}'],
    },
  },
});
