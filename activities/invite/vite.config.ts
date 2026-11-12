import { defineConfig } from 'vite';

export default defineConfig({
  base: '/',
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    // Explicit tunnel hostname only. Never allow every Host header.
    allowedHosts: process.env.ACTIVITY_TUNNEL_HOST
      ? [process.env.ACTIVITY_TUNNEL_HOST]
      : [],
  },
});
