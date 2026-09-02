import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react-swc'
import path from 'node:path'

/**
 * The UI face is only discoverable once the stylesheet has downloaded and
 * parsed, which puts the one font every screen draws a full round trip behind
 * the HTML. Only the latin variable file is preloaded: latin-ext and the mono
 * weights are conditional on what a page actually renders, and preloading a
 * font that never gets drawn is a wasted request.
 */
function preloadUiFont(): Plugin {
  const uiFont = /^assets\/hanken-grotesk-latin-var-[^/]+\.woff2$/
  return {
    name: 'jinn-preload-ui-font',
    apply: 'build',
    transformIndexHtml: {
      order: 'post',
      handler(html, ctx) {
        const emitted = Object.keys(ctx.bundle ?? {}).find((name) => uiFont.test(name))
        if (!emitted) {
          throw new Error(
            'jinn-preload-ui-font: no hanken-grotesk-latin-var woff2 in the bundle. ' +
              'If the file was renamed, update the pattern; if the face was dropped, drop this plugin.',
          )
        }
        return {
          html,
          tags: [
            {
              tag: 'link',
              // Fonts are fetched in CORS mode even same-origin; without
              // crossorigin the preload misses and the browser fetches twice.
              attrs: { rel: 'preload', as: 'font', type: 'font/woff2', crossorigin: '', href: `/${emitted}` },
              injectTo: 'head-prepend',
            },
          ],
        }
      },
    },
  }
}

export default defineConfig(() => {
  const gatewayPort = process.env.GATEWAY_PORT ?? '7777'
  return {
    plugins: [
      react(),
      preloadUiFont(),
    ],
    resolve: {
      alias: {
        '@': path.resolve(__dirname, 'src'),
        // The plugin SDK is a specifier, not a package: a real package would
        // need its own build and its own React peer, and the singleton the SDK
        // exists to guarantee is exactly what a second React copy would break.
        '@jinn/plugin-sdk': path.resolve(__dirname, 'src/plugins/sdk/index.ts'),
        // The event vocabulary the shell's transport types against, ported
        // verbatim under web/ (UI-1; UI-3 retires it).
        '@jinn/gateway-events': path.resolve(__dirname, 'gateway-events/src/index.ts'),
        // This one does resolve at build time: the module is runtime code the
        // bundle really carries. It is a pure leaf with an empty import list,
        // which is what keeps that safe with no polyfills.
        '@jinn/fallback-map-wire': path.resolve(__dirname, 'vendor/fallback-map-wire.ts'),
        // The same leaf treatment as the line above, and for the same reason: the
        // editor has to judge a model id by the rule the config loader judges it
        // by, and a second copy of that rule is a second answer waiting to drift.
        '@jinn/model-id': path.resolve(__dirname, 'vendor/model-id.ts'),
      },
    },
    build: {
      outDir: 'out',
      emptyOutDir: true,
      sourcemap: false,
      rollupOptions: {
        output: {
          manualChunks(id) {
            const normalized = id.split(path.sep).join('/')
            if (!normalized.includes('/node_modules/')) return
            if (
              normalized.includes('/node_modules/react/') ||
              normalized.includes('/node_modules/react-dom/') ||
              normalized.includes('/node_modules/scheduler/')
            ) {
              return 'vendor-react'
            }
            if (
              normalized.includes('/node_modules/react-router/') ||
              normalized.includes('/node_modules/react-router-dom/')
            ) {
              return 'vendor-router'
            }
            if (
              normalized.includes('/node_modules/@tanstack/react-query/') ||
              normalized.includes('/node_modules/@tanstack/query-core/')
            ) {
              return 'vendor-query'
            }
            // Radix and cmdk deliberately get no bucket. One shared bucket is
            // all-or-nothing: a single primitive in the shell drags every
            // primitive any route uses into the first load. Left alone, Rollup
            // puts each primitive with the route that needs it.
          },
        },
      },
    },
    server: {
      proxy: {
        '/v1': {
          target: `http://127.0.0.1:${gatewayPort}`,
          changeOrigin: true,
        },
      },
    },
  }
})
