# Web UI port inventory

**What this is.** A read-only survey of the existing Jinn web app, produced so the port into
this repo is a series of choices rather than archaeology done mid-packet. No code was written
and nothing was ported in producing it.

**The direction it serves.** The web UI is ported into `jinn-harness` - one repo, Rust
workspace, the TypeScript client inside it, served by a plugin, its bundle pinned as a
content-addressed artifact - and made totally malleable. The UI is a RENDERER of a plugin tree,
never an extension API; a user extension is a waterfall listener on the kernel's event bus;
the user-facing tier is JS running inside a WASM plugin, so the constitution's
all-plugins-are-WASM law stands unamended.

**The instruction it obeys.** Port the existing UI and its quirks as-is rather than reinventing
it: "this way we don't reinvent it and have to fix all of the bugs we banged our heads against
the wall for. we just make it malleable." Section 2 is therefore the deliverable, not
documentation overhead.

**The line it makes actionable.** Port the pixels and the fixes, not the spine. The view layer
and its hard-won behaviour come across close to verbatim. The data/state layer cannot - it
speaks the old gateway's REST surface and this repo has a different API seam. And a wholesale
copy with extension points bolted on afterwards produces a monolith with hooks, which is the
opposite of the goal.

**Reading order.** Section 2 (the quirks register) and section 3 (coupling) are the two that
change decisions. Section 1 is the map you need to read them. Sections 4 and 6 are the forward
plan. Section 5 is the pin; read it first if this document is more than a few weeks old.

**Honesty rule applied throughout.** Where a carried behaviour's reason could not be
reconstructed it is marked `carried deliberately, reason unknown`, and those items are
collected into explicit lists (2.13, 2.25). A workaround whose reason nobody recorded is a
claim that is right for a reason nothing enforces; we will not be able to tell later whether it
is still needed, or whether this repo even has that problem. Those lists are a deliverable and
a known limit, not a gap to be filled with plausible guesses.

---

## Contents

1. Surface map
2. The quirks register - part I (chat, rendering, scroll, typography), part II (build, platform, routing, state, non-chat surfaces)
3. Coupling report
4. Malleability read
5. The pin
6. Toolchain and gate implications

## 1. Surface map

All counts are `wc -l` on `.ts`/`.tsx`/`.css` under `packages/web/src/` at the pinned sha.

### 1.1 Route surfaces

Routes are declared once in `packages/web/src/lib/app-routes.ts` (id, path, availability, semantic surface) and bound to elements in `packages/web/src/main.tsx` via `lazyRoute()`. Every entry below is a lazy chunk except the two redirects.

| Surface | Route path | LOC | What it is | Main components | Plugin-entry candidate? |
|---|---|---|---|---|---|
| Chat | `/` | 19,960 | The multi-pane conversation workspace: working set, grid layout, drag/drop pane placement, sidebar session list, composer, transcript virtualizer, TTS/mic | `routes/chat/page.tsx` (996), `components/chat/chat-sidebar.tsx` (1,767), `chat-messages.tsx` (1,334), `chat-input.tsx` (1,046), `chat-pane.tsx` (547), `cli-terminal.tsx` (578) | **partial** - the surface is a plugin entry, but it owns the app's default route and half the shared vocabulary (markdown, media, mic, session state) |
| Chat legacy redirect | `/chat/:sessionId?` | 13 | Rewrites old per-session URLs onto `/?session=` | `routes/chat/legacy-chat-redirect.tsx` | no - compat shim |
| Cron | `/cron`, `/cron/:id` | 1,547 | Job list + detail with run history, schedule editor, delete flow | `routes/cron/page.tsx` (336), `detail.tsx` (254), `components/crons/weekly-schedule.tsx` (388) | **yes** - self-contained data + UI, one gateway domain |
| Todos | `/todos`, `/todos/b/:board`, `/todos/:todoId` (+ `/kanban` redirect) | 12,701 | The work ledger: kanban board (2,724), full task page (4,403), list/virtualizer (704), quick-add capture (569), pickers (1,005), filter bar, needs-you view; plus `components/peek` (651) | `board/board-page.tsx` (891), `task-page/task-page.tsx` (586), `task-page/activity.tsx` (556), `filter-bar.tsx` (504), `needs-you-view.tsx` (503) | **yes** - largest single domain surface; already hosts two contribution areas (`todo.detail.actions`, `todo.detail.sections`) |
| Notes | `/notes`, `/notes/*` | 1,391 | Markdown knowledge browser: folder/note sidebar, editor, dictation | `notes/note-editor.tsx` (399), `note-list.tsx` (202), `page.tsx` (227) | **yes** - already feature-gated (`availability: "notes-enabled"`), which is a plugin toggle in all but name |
| Experiments | `/experiments`, `/experiments/:id` | 763 | Hypothesis list + detail with readings chart, record-reading and conclude dialogs | `experiments/detail.tsx` (219), `reading-chart.tsx` (94) | **yes** - smallest complete CRUD surface; the natural first port |
| Activity / Logs | `/logs` | 467 | Live gateway log stream with summary cards and a filterable browser | `logs/page.tsx` (218), `components/activity/log-browser.tsx` (249) | **yes** - one data source, no shared state |
| Limits | `/limits` | 460 | Engine rate-limit windows with freshness derivation | `limits/page.tsx` (241), `use-engine-limits.ts` (219) | **yes** - read-only, one endpoint |
| Organization | `/org` | 1,429 | Employee tree on a React Flow canvas (d3-tree layout + dagre fallback), detail/edit panel | `components/org/org-map.tsx`, `employee-detail.tsx` (284), `employee-editor.tsx` (291), `layout/d3-tree-layout.ts` (249) | **yes** - heavy but isolated; only external pull is the model-selector row from chat |
| Settings | `/settings` | 3,890 | Config editor: appearance, engines + model-chain editor (951), voice/realtime, STT, shortcuts, pairing | `settings/page.tsx` (1,427), `engines/model-map-editor.tsx` (241), `voice-section.tsx` (291), `stt-section.tsx` (280) | **partial** - a plugin-tree host would need settings to stay core (it writes `config.yaml` and drives theming/accent), but per-domain panels should become plugin-contributed sections |
| Plugin settings | `/settings/plugins` | 470 | Plugin inventory: enable/disable, reveal on disk, rescan, watcher status | `settings/plugins/page.tsx` (103), `inventory.ts` (118), `plugin-row.tsx` (118) | **partial** - this is the plugin manager itself; it must live in core |
| Skills | `/skills`, `/skills/:name` | 443 | Skill catalogue list + full-page markdown view/edit | `skills/detail.tsx` (293), `page.tsx` (150) | **yes** - read-mostly, one endpoint |
| File | `/file` | 42 | Standalone syntax-highlighted file viewer (thin wrapper around chat's `file-view.tsx`, 320) | `routes/file/page.tsx` | **partial** - the viewer belongs in a shared preview layer, not a plugin |
| More | `/more` | 279 | Mobile overflow screen: grouped nav destinations, theme cycle, workspaces group | `more/page.tsx` (153), `workspaces-group.tsx` (126) | no - nav chrome; it renders whatever the nav registry holds |
| Workflows | `/workflow`, `/workflow/:id`, `/workflow/:id/runs/:runId` | 5,411 | Workflow list, React Flow graph editor (3,321) with node palette/inspector, run canvas + run inspector, approval decisions, lifecycle menu | `editor/inspector.tsx` (753), `editor/node-card.tsx` (341), `run-inspector.tsx` (387), `page.tsx` (298) | **yes** - second-largest domain, fully self-contained graph tooling |
| Talk orb harness | `/talk-orb` | 437 | Screenshot bench for orb variants and Talk tool calls, on fixtures | `talk-orb-harness/page.tsx` (193), `tool-bench.tsx` (153) | **yes** - a dev/test surface; ideal plugin |
| Redesign | `/redesign` (DEV only) | 207 | Static "Ledger Dock" mockup with hardcoded sample data, no gateway calls | `redesign/page.tsx` | **yes** - or drop it; it is dead weight in a port |
| Contributed plugin page | `/*` (splat, last) | 202 | Host for the existing `routes` contribution area | `routes/contributed-route.tsx` | n/a - this *is* the current plugin door |

### 1.2 Shared component families

| Family | LOC | What it provides | Who consumes it | Verdict |
|---|---|---|---|---|
| `components/ui` | 2,520 | Radix-based primitives: button, card, dialog, dropdown, select, tabs, tooltip, command, context-menu, image-lightbox (324), video-player, icon registry, employee avatar/chip | 16 surfaces + the plugin SDK re-exports it wholesale | **cross-cutting - core.** It is already the plugin design system. |
| `components/chat` | 15,146 | Nominally chat, actually two things: the chat surface proper, and ~10 modules everything else depends on - `markdown-view` peers, `file-view`, `mic-waveform`, `chat-input`, `mobile-tab-bar`, `session-row-menu`, `chat-route-helpers`, `todo-prefix-context`, `message-send-state`, `transcript-open` | Chat, plus notes, todos, cron, workflow, file, org, talk, plugin SDK bridge, and two shared hooks | **split required.** Body belongs to the chat surface; the ~10 cross-imported modules are core in disguise. |
| `components/talk` | 10,810 | The voice overlay runtime: WebRTC transport (`transport/`, ~2,300), tool registry and executors (`tools/`, ~2,400), page-context snapshotting (`context/`, ~1,600), orb rendering (~900), situation sheet/renderers | Mounted above the router in `client-providers.tsx`; reads every surface through `context/surface-adapters.ts` | **cross-cutting - core (or its own privileged plugin).** It has a structural dependency on knowing what every surface is showing; a plugin tree changes the shape of that contract. |
| `components/global-search` (+ `global-search.tsx`) | 1,541 | Cmd-K palette: result list, preview pane, verbs, quick-create, todo workbench, keyboard handling | Only `page-layout.tsx` (lazy) | **cross-cutting - core**, but it hardcodes per-kind knowledge (`kind-meta.ts`, `static-pages.tsx`) that a plugin tree would need to make contributable. |
| `components/shell` | 379 | `PageScaffold`, `LargeTitleHeader`, `PrimaryAction` - the large-title/collapse chrome contract | cron, experiments, limits, more, settings, skills, todos, workflow | **cross-cutting - core.** This is the page contract every ported surface must honour. |
| `components/edge-back` | 616 | iOS-style edge-back gesture, previous-view snapshotting, coarse-pointer detection | `page-layout.tsx`, `chat/use-swipe-actions.ts` | **cross-cutting - core** (navigation gesture layer). |
| `components/auth` | 766 | Pairing screens (browser + native), remote-access panel | `main.tsx`, `auth-provider`, settings, more, workspaces | **cross-cutting - core** (renders *before* any surface exists). |
| `components/workspaces` | 464 | Workspace switcher menu, create dialog, native switcher | `status-bar.tsx` (as a core contribution), `/more` | **cross-cutting - core** (multi-instance identity). |
| `components/peek` | 651 | Slide-over peek panel + todo peek | chat, global-search, todo-mention | **belongs to Todos** with a generic panel shell left in core. |
| `components/org` | 1,242 | React Flow employee nodes and tree layout | `/org` only | **one surface.** |
| `components/crons` | 388 | Weekly schedule picker | `/cron` only | **one surface.** |
| `components/activity` | 249 | Log line parser + browser | `/logs` and `live-stream-widget` | **one surface** (+ one widget). |
| `components/stt` | 87 | Whisper model download modal | chat, notes, todos | **cross-cutting - small; core** (paired with `use-stt`, 452). |
| Loose top-level | 3,210 | `onboarding-wizard` (780), `cli-terminal` (578), `live-stream-widget` (309), `markdown-view` (299), `pill-nav` (266), `page-layout` (181), `attachment-ref-preview` (160), `todo-mention` (111), `status-bar` (88), `code-block-chrome`, `route-loading`, `search-overlay-context`, `emoji-favicon`, `todo-glance` | mixed | **Core:** page-layout, pill-nav, status-bar, route-loading, search-overlay-context, emoji-favicon, markdown-view, onboarding-wizard. **Surface-owned:** cli-terminal (chat), todo-mention/todo-glance/attachment-ref-preview (todos), live-stream-widget (activity). |

Hooks (`src/hooks`, 3,802) group the same way: `use-live-session` (1,428), `use-chat-tabs` (337), `use-sessions` (215), `use-stick-to-bottom` (293), `use-pins` (74) are chat; `use-cron`, `use-skills`, `use-departments`, `use-employees`, `use-scroll-anchor` are single-surface; `use-gateway` (139), `use-query-invalidation` (273), `use-features` (30), `use-keyboard-shortcuts` (71), `use-idle-mount`, `use-root-css-variables`, `use-onboarding`, `use-page-visibility` are core; `use-stt` (452) and `use-file-drop` (67) are three-surface shared.

### 1.3 The existing plugin/contribution system

There are two layers, and they are already close to a plugin tree in miniature.

**Layer A - the contribution registry (`src/contrib/`, 330 LOC).** `registry.ts` is a singleton keyed by dotted area id, with per-area referentially-stable snapshots and per-area subscriptions (so a status-bar registration does not re-render the routes area). `slot.tsx` renders one area's contributions, each wrapped in `boundary.tsx` - a class error boundary with a Retry that remounts, in `chip` or `pane` variant. Provenance is stamped by the registry, never read off the author's object (`registry.ts` `put()`): a contribution cannot claim `source: "core"`.

The area vocabulary is fixed and small - seven ids, declared twice (`src/contrib/types.ts` and `src/plugins/sdk/areas.ts`, held in sync by a contract test):

| Area | Host file | Live consumers |
|---|---|---|
| `routes` | `src/routes/contributed-route.tsx` | plugin pages |
| `sidebar.nav` | `src/lib/nav.ts` + `src/lib/use-navigation.ts` | nav rail, mobile tab bar, `/more` |
| `statusbar.right` | `src/components/status-bar.tsx` | two **core** contributions (workspace switcher, theme toggle) - core already dogfoods the mechanism |
| `todo.detail.actions` | `src/routes/todos/task-page/crumb-bar.tsx` | plugins only |
| `todo.detail.sections` | `src/routes/todos/task-page/task-page.tsx:465` | plugins only |
| `chat.composer` | `src/components/chat/chat-pane.tsx:491` | plugins only |
| `home.widgets` | `src/components/chat/chat-sidebar.tsx:1636` | plugins only |

**Layer B - disk plugins and the SDK (`src/plugins/`, 2,578 LOC).**
- `disk-plugins.ts` (178) reconciles against `GET /api/plugins`: the gateway discovers `<jinn-home>/plugins/<id>/` and serves only the client half of plugins enabled in config, so being served *is* the enablement decision - the dashboard keeps no enablement state. It distinguishes 422 (installed but won't compile → keep the running copy, warn) from every other failure (gone → unload), tracks folder-id vs `plugin.id` drift, and publishes a `settled` flag so a deep link to a plugin page doesn't bounce to `/` before the first scan.
- `disk-plugins-bridge.tsx` rescans on the `plugins:changed` gateway event and on every socket reconnect - hot reload without a page refresh.
- `runtime-loader.ts` (256) is the no-build ESM door: source → mask comments (`codeOnly`) → reject unsupported bare specifiers → rewrite only mapped import specifiers to blob shim URLs → blob `import()` → validate `{ id, name?, register }` → `register(ctx)`. The import allowlist is exactly three specifiers (`@jinn/plugin-sdk`, `react`, `react/jsx-runtime`, plus `sdk/runtime.ts`) so a plugin cannot resolve a second React.
- `plugin-context.ts` (206) hands each plugin a scoped context: `contribute`/`contributeMany` (ids namespaced `<pluginId>:<id>`, source stamped), `onDispose`, `storage` namespaced under `jinn.plugin.<id>.`, `backend(suffix)` → `/api/plugins/<id><suffix>` with a hardened `..`-segment refusal (including `%2e` and backslash separators), and `events()` → a WebSocket on `/api/plugins/<id>/events`.
- `sdk/` (1,300 + 628 LOC of hand-authored `.d.ts`) exports the app's own React and ~40 UI primitives, plus a three-tier `host` API: readonly `state` (activeSession, gatewayStatus), `onEvent`/`navigate`/`notify`, and 16 typed verbs across todos, sessions, employees, workflows, notes, connectors, cron, knowledge - each passing `assertVerbAllowed` (`host-permissions.ts`) before one `authFetch` to an endpoint the dashboard already calls. Contract version `1.2.0`, versioned independently of the app.

**Route contribution semantics** (`contributed-route.tsx`): mounted last on the `*` splat, so React Router matches every app route first and shadowing is structurally impossible. Paths are parsed into static/`:param` segments, must begin with a static segment, forbid wildcards and duplicate params; reserved first segments are derived from `APP_ROUTES` rather than a second list. Literal beats capture on ties; equal ties go to first-registered. The host supplies `PageLayout` and the scroll container - a plugin page cannot draw its own chrome, and `PageLayout` is deliberately not on the SDK export list.

**Constraints.**
1. **Seven fixed areas, hardcoded hosts.** Every slot is a `<Slot area={…}>` hand-placed in a core file. There is no tree, no nesting, no ordering relative to core items (contributed nav rows are appended *after* core rows by construction, `nav.ts`).
2. **No capability boundary.** `runtime-loader.ts` says it outright: a plugin is evaluated as ESM in the dashboard's own realm with the app's full authority. The boundary is error isolation only. `host-permissions.ts` grants all 16 verbs today; the gate exists so it can be narrowed later.
3. **Core surfaces are not plugins.** All 18 route surfaces are static imports in `main.tsx`; only the splat is contributable. `statusbar.right` is the single area where core registers through the same mechanism a plugin does.
4. **One-way data flow.** A contribution renders and calls host verbs; nothing lets a plugin *replace* or wrap a core surface, decorate a route, or intercept navigation.
5. **Route contributions are flat.** Single-level path matching in `contributedRouteFor`; no nested routes, no layout routes, no loaders.

**Could a plugin tree subsume it?** Yes, and the registry is the right seed. `contrib/registry.ts` (area id → ordered, provenance-stamped, individually-boundaried entries) generalizes to a tree by replacing the flat dotted-string key with a path and letting a node's children be an area. `plugin-context.ts`'s namespacing/disposal discipline, `boundary.tsx`'s isolation, `runtime-loader.ts`'s import allowlist, and `host-permissions.ts`'s verb gate all survive unchanged. What must be rebuilt: `main.tsx`'s static route table and `app-routes.ts` become tree data rather than a literal; `contributed-route.tsx`'s "last, on the splat, and never shadow" rule dissolves once core surfaces are themselves nodes; and the seven-area vocabulary becomes a node contract instead of a string enum.

### 1.4 The shell - what is left after subtracting every surface

Roughly **15,600 LOC**. This is the irreducible core a plugin-tree renderer would still have to provide:

- **Bootstrap and routing** - `src/main.tsx` (262): lazy route table, prefetch registration, idle prefetch of chat + todos, app error boundary with a Refresh screen, service-worker registration (prod, non-native), Talk and plugin navigator handles, native-gateway pairing gate before the router mounts. Plus `lib/app-routes.ts` (53), `lib/lazy-route.ts` (75, with chunk-reload retry on stale deploys), `lib/route-prefetch.ts` (9).
- **Provider stack** - `routes/client-providers.tsx` (78) nests, in order: QueryClient → Theme → Auth → **AuthGate** → Settings → Gateway → todo-prefix context, with the Talk overlay, document title, favicon, query-invalidation bridge, `PluginNotices`, `PluginHostBridge` and `DiskPluginsBridge` mounted as siblings above the router. `providers.tsx` (73, theme + PWA theme-color sync), `auth-provider.tsx` (164), `settings-provider.tsx` (296).
- **Auth gate** - `components/auth` (766) + `lib/auth.ts` (178): pairing code/token flows, local bootstrap, device list, cross-workspace launch-code consumption from the URL fragment.
- **Layout and nav** - `components/page-layout.tsx` (181), `pill-nav.tsx` (266, the desktop rail), `chat/mobile-tab-bar.tsx` (100), `status-bar.tsx` (88), `components/shell/` (379), `components/edge-back/` (616), `route-loading.tsx`, `search-overlay-context.tsx`, `lib/nav.ts` (111) + `lib/use-navigation.ts` (16).
- **Theming** - `routes/globals.css` (1,557, the whole token system), `lib/themes.ts`, `hooks/use-root-css-variables.ts`.
- **Cross-app overlays** - global search (1,541), onboarding wizard (780), plugin notice stack (135), live-stream widget (309), emoji favicon (34).
- **Transport** - `lib/api.ts` (1,075), `lib/ws.ts` + backoff (215), `lib/gateway-transport.ts` (115), native-gateway profiles/socket/transport/bootstrap (675), `query-client`/`query-keys` (71), `hooks/use-gateway.tsx` (139), `hooks/use-query-invalidation.ts` (273).
- **Platform adapters** - `src/platform/` (691): a `contracts.ts` interface with web (287), Tauri, lazy-Tauri, fallback and test adapters; `native-bridge.ts` decides web vs native at boot.
- **Plugin machinery** - `src/contrib/` (330) + `src/plugins/` (2,578), described above.

### 1.5 Size ledger

| Slice | Files | LOC |
|---|---|---|
| **All `src` (`.ts`/`.tsx`/`.css`)** | 1,037 | **148,721** |
| Tests (`*.test.*` + `__tests__/`) | 423 | 63,068 |
| **Production source** | **614** | **85,653** |
| - `src/components` | 270 | 37,820 |
| - `src/routes` | 210 | 34,013 (incl. `globals.css` 1,557) |
| - `src/lib` | 57 | 5,893 (≈3,083 core transport/routing, ≈2,810 surface-owned domain logic) |
| - `src/hooks` | 25 | 3,802 |
| - `src/plugins` | 31 | 2,578 (incl. 628 of hand-authored `.d.ts`) |
| - `src/platform` | 13 | 691 |
| - `src/contrib` | 5 | 330 |
| - `src/test` (helpers) | 2 | 264 |
| - `src/main.tsx` | 1 | 262 |

Rebalanced by ownership: **shell/core ≈ 15,600**; **surface code ≈ 70,000**, of which the three heavyweights are Chat ≈ 19,960, Todos ≈ 12,700 and Workflows ≈ 5,400, with Talk's 10,810 sitting outside the route system entirely as an always-mounted overlay. Test-to-source ratio is 0.74:1.

Two things a porter should know up front: `globals.css` (1,557 lines) is a single token file every surface reads, so it moves as one unit or not at all; and `components/chat` cannot be ported as one surface - ten of its modules are imported by seven other surfaces and by core hooks, so the chat port begins with an extraction, not a move.
## 2. The quirks register

The valuable half of this document. Every entry is a non-obvious behaviour in the view
layer, with **what it fixed**. This repo's commit messages are unusually explicit about
failure modes, so most entries are commit-cited with a reproduced defect named in the
message. Where a reason could not be reconstructed it is marked
`carried deliberately, reason unknown` and listed again at the end of each part - those
lists are themselves a deliverable and a known limit.

Confidence values: `commit-cited` (a commit body names the defect) · `test-enforced` (a
test in the tree fails if the quirk is removed) · `comment-only` (a source comment states
the mechanism, no commit body) · `carried deliberately, reason unknown`.

---

### Part I - chat, rendering, scroll, typography

#### 2.1 Scroll, stick-to-bottom, and the transcript open

| # | Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|---|
| A1 | Follow/auto-scroll is flipped **only** by a user `scroll` event, and detaches on *any* increase in distance-from-bottom - not on crossing the 56px threshold. The threshold's only job is deciding when to *re*-engage. | `hooks/use-stick-to-bottom.ts` (`onScroll`), `hooks/stick-geometry.ts` (`followAfterScroll`) | A freshly opened transcript keeps resizing (arrival/fold animations, highlighting, image decode) and every content-ResizeObserver fire re-pinned the reader to the bottom. Only a gesture big enough to clear the 56px band in one step escaped; small scroll-ups were silently undone. | `b479b77b` "Detach chat follow on any scroll away from the bottom" | commit-cited |
| A2 | The decision uses **distance from bottom**, never `scrollTop` direction. | same, `onScroll` | Content shrinking *above* makes the browser clamp `scrollTop` down while the view is still at the bottom; a direction check detached a live stream there. | `b479b77b` (commit body states it) | commit-cited |
| A3 | The jump-to-latest arrow is a **second, independent decision** over the same scroll event, gated on the 56px gap alone, while follow-detach is gated on movement. | `use-stick-to-bottom.ts` (`showArrow`) | A 4px drag detached follow and then offered to scroll the reader 4px. | `42a5bfd4` "Open a chat at the bottom and stop writing over the reader's scroll" | commit-cited |
| A4 | The smooth-scroll suppression flag (`animatingRef`) has **two** exits: reaching the threshold, *or* any event that widens the gap. | `use-stick-to-bottom.ts` | REPRO 2 in the commit: jump-to-latest during a stream, then scroll up with keyboard/scrollbar → nothing ever detached again. The flag only cleared on arrival-within-threshold, and content growing past the animation target meant it never arrived, so it latched permanently and swallowed every later scroll. | `0dde83d6` "Hold the read position, the bottom, and the transcript while streaming" (REPRO 2) | commit-cited |
| A5 | Following is performed **synchronously in `useLayoutEffect`**, keyed on growing content - never in a rAF or IntersectionObserver. | `use-stick-to-bottom.ts` (growth effect) | The old IntersectionObserver(position) + ResizeObserver(content)→rAF pair raced: one render grew content past the 80px sentinel band, the IO flipped "at bottom" false before the queued rAF read it, auto-scroll never resumed. Reproduced ~8700px adrift. | `6b767264` "rebuild scroll-to-bottom - useStickToBottom hook (fixes streaming detach)" | commit-cited |
| A6 | `containerRef` is a **callback ref**, not a ref object. | `use-stick-to-bottom.ts` | The scroller mounts in a later render than the hook (the empty-state branch renders first), so listener effects never re-ran and never attached. | `6b767264` | commit-cited |
| A7 | Tab-return re-sync listens to `visibilitychange` **and** `pageshow`, not rAF. | `use-stick-to-bottom.ts` | rAF is throttled in background tabs, so a rAF-based resync never fired on return. | `6b767264` | commit-cited |
| A8 | When NOT following, the hook never writes `scrollTop` at all, deliberately delegating to the browser's native `overflow-anchor`. | `use-stick-to-bottom.ts` (file header) | Preserves the read position through image/content reflow above without fighting the browser. Note: `lib/scroll-anchor.ts` records that **Safari implements no `overflow-anchor`**, which is why the manual anchoring exists in parallel. | `6b767264`; source comment in `lib/scroll-anchor.ts` header | commit-cited |
| A9 | `latestMessageKey` is passed alongside `messageCount`: a count that grows while the key is unchanged is treated as a **prepend**, and the "seen" baseline is advanced instead of the unread badge. | `use-stick-to-bottom.ts` (growth layout effect) | Loading an older history page incremented the unread badge as if new replies had arrived. | `47125862` "Fix unread badge during history prepend" (subject only; body empty) | commit-cited |
| A10 | Opening a transcript writes **one** target chosen before paint (remembered offset, else bottom) - never bottom-then-correct - followed by a **bounded settle window** (400ms / first unchanged content size) that only re-pins while still pinned. | `components/chat/transcript-open.ts` | The open decided its position twice: a mount snap `scrollTop = scrollHeight` against estimated row heights, then a remembered offset two frames later from a rAF. The browser painted in between - that paint is the visible jump; and against estimates the first write lands short, which is the open that needs a manual tap on the down-arrow. | `42a5bfd4` | commit-cited |
| A11 | The settle window's opening commit **skips its own size comparison** (`phase === 'opening'` returns early). | `transcript-open.ts` `useSettleWindow` | The opening commit runs the effect too, and its size reading is the one just recorded - comparing them closed the window before it opened. | source comment; module introduced by `42a5bfd4` | comment-only (mechanism), commit-cited (module) |
| A12 | A guard that closed the settle window when *another* part of the hook pinned the bottom was **removed**, explicitly labelled a disproved hypothesis. | `transcript-open.ts` | The guard read another pin as "the reader took over" and closed the window early. Browser QA on ten consecutive opens found the windowed transcript landing 1389px short of the bottom and staying there. | `b99a0178` "Let the virtualizer finish an open, and stop it hijacking the reader" | commit-cited |
| A13 | A phone only remembers a scroll position it can **actually read now**. | position-remembering path feeding `initialScrollTop` | A phone remembers the wrong position for a chat it is leaving: the thread is display-toggled, so the scroller is hidden and reports `scrollTop 0`, and the next open restored the top of history. | `b99a0178` | commit-cited |
| A14 | `STICK_THRESHOLD_PX = 56` (was 80 in the original design). | `hooks/stick-geometry.ts` | The 80px sentinel band is what the old IO design lost the stream inside. The arithmetic was split out of the hook purely because the hook broke the repo's 300-line cap. | `b99a0178` (states the extraction reason); `6b767264` (80px band) | commit-cited |

#### 2.2 Virtualisation

| # | Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|---|
| B1 | The virtual row is a **`RenderGroup`, not a message**; a fold region is one item, and a *folded* region is told to drop its evidence rather than mount it. The virtualize threshold counts **rows**, not groups. | `components/chat/transcript-virtualizer.ts` (header), `VIRTUALIZE_THRESHOLD = 50` | A single long turn is one group, so windowing by group alone would still mount all 500 rows. 500 messages → 15 mounted rows, 0 row bodies on append (was 501), 0 per streaming token. | `9352ae29` "Virtualise the chat transcript" | commit-cited |
| B2 | `getItemKey` must be a **stable group id**, never an index. | `transcript-virtualizer.ts` `groupKey` | Prepending an older page shifts every index; an index-keyed measurement cache then describes the wrong rows, anchoring lands wrong and the reader jumps. | `9352ae29` | commit-cited |
| B3 | The gap above the virtual block is **measured** and declared as `scrollMargin`. | `components/chat/virtual-block-offset.ts` + `transcript-virtualizer.ts` header | Without it, a row's `start` (spacer coordinates) is compared to the scroller's real `scrollTop` - two coordinate systems off by exactly that gap - so every row in the top ~80px reads as "above the reader" and takes a scroll correction the reader watches happen. | `b626223a` "Give the reader their flick back on every long list" (same fix applied to the Todo list; its header comment "argued the opposite and is rewritten") | commit-cited |
| B4 | **Only** the write `scrollTranscriptTo` is making right now reaches the scroller; the virtualizer's own rAF `scrollToIndex` retries are suppressed. Its resize-compensation writes (`adjustments !== 0`) still pass. | `transcript-virtualizer.ts` `transcriptScrollTo` / `scrolling` WeakSet | `scrollToIndex` re-issues from rAF for up to five seconds and never asks who owns the position: it landed after the browser painted the frame it corrects (carrying the last 1373px of a 140-message open as a visible jump), and it dragged back readers who had scrolled away. | `cd069a89` "Stop the transcript's own measuring from reading as the reader scrolling"; the earlier over-correction is `42a5bfd4`, whose regression is named in `b99a0178` | commit-cited |
| B5 | Every transcript write records where it left the scroller (`writtenTop`); a scroll event landing exactly there **decides nothing** - and the answer is **spent once given**. | `transcript-virtualizer.ts` `takeTranscriptWriteTop`, consumed in `use-stick-to-bottom.ts` | Resize compensation fires dozens of times while a windowed transcript measures on open; read as user intent they detached a reader who never touched anything (open stopped 1373px short). Not spending it would also swallow a later genuine scroll landing on the same pixel - and for a transcript that pixel is the bottom, the one place being misread costs follow. | `cd069a89` | commit-cited |
| B6 | Re-engaging follow requires a move **toward** the bottom; an event that moves the position without changing the gap re-engages nothing. | `hooks/stick-geometry.ts` `followAfterScroll` | A fold's anchoring compensation (which moves position but holds the gap) handed follow back to a reader who had not asked for it. | `cd069a89` | commit-cited |
| B7 | Restoring after a prepend takes **two commits**: coarse scroll through the virtualizer's own offsets, then a settle off the anchored row's rect. The first pass returns `false` so the measured-rect fallback does not correct a second time on top of it. | `transcript-virtualizer.ts` `applyTranscriptAnchor` / `restoreVirtualAnchor` / `alignAnchoredRow` | The anchored row is ~100 rows above the window by then, unmounted, with no rect to measure; and rows entering the window on the way there resolve their real heights as they arrive, moving everything below them again. | `9352ae29` | commit-cited |
| B8 | Restoring goes through `virtualizer.scrollToOffset`, never a raw `scrollTop` assignment. | `restoreVirtualAnchor` | The virtualizer has to know the position moved, or the window it renders stays the one computed for the old offset and the reader lands among the just-inserted rows. | source comment; module introduced by `9352ae29` | comment-only |
| B9 | `content-visibility: auto` / `contain-intrinsic-size` is **deliberately absent** from message rows, and a test asserts both style properties are empty strings. | `components/chat/__tests__/message-row-content-visibility.test.tsx`; absence in `chat-messages.tsx` | A flat 120px `contain-intrinsic-size` on every row meant each re-measured the first time it came into view, changing the height above the reader mid-scroll - the drag "going sticky" is that shift fighting the finger. Windowed, the virtualizer's ResizeObserver would cache the placeholder height for every row and misplace every offset below it. | test file header + `8ab571b2` "Stop a send from folding away what the reader can see" | test-enforced + commit-cited |
| B10 | Expansion state (expanded tool group, "Show more" user paste) lives in a **Map on the transcript keyed by message id**, not `useState` in the row, and deliberately is not React state. | `components/chat/transcript-expansion.ts` | A virtualised transcript unmounts scrolled-past rows and a row-local `useState` dies with it - expansions silently re-closed every time a row left the window. Parent state would re-render the whole window. | `9352ae29` (lists it as one of "four things that had to survive") | commit-cited |

#### 2.3 Touch and momentum (WebKit)

| # | Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|---|
| C1 | There is **no `touch-action` on `html`**, and the CSS carries a comment forbidding one. Only the edge-back gutter and `.xterm` declare their own. | `routes/globals.css` (~line 657) | `touch-action: manipulation` on `html` takes inertial scrolling away from **every** scroller below it in WebKit - three long lists stuck to the finger and stopped dead on release. It existed for the 350ms double-tap delay, which `user-scalable=no` in the viewport meta already removes, so it bought nothing. | `b626223a` | commit-cited |
| C2 | Non-deliberate scroll corrections are **held** while a touch drag or its momentum owns the scroller, and applied as one summed delta after a 120ms quiet window. | `components/chat/touch-scroll-phase.ts`; consumed in `transcriptScrollTo` | Assigning `scrollTop` ends WebKit's momentum on the spot, so a re-measure correction mid-fling reads as the list stopping dead on finger-lift. | `b626223a` | commit-cited |
| C3 | The held value is banked as an **increment** (`adjustment - counted`), and `counted` resets on every scroll event. | `touch-scroll-phase.ts` `holdScrollAdjustment` / `onScroll` | The virtualizer's `adjustments` is a running total since the scroller last reported a position, restarting at zero each scroll event; adding the value itself counts each earlier row again on every row after it. | source comment (detailed mechanism); module from `b626223a` | comment-only (arithmetic), commit-cited (module) |
| C4 | The touch phase stays open **past `touchend`** for as long as scroll events keep arriving. | `touch-scroll-phase.ts` | Momentum is still running after the lift, and the stretch after the lift is the whole of what the reader notices. | source comment + `b626223a` | commit-cited |
| C5 | `html, body { height:100%; overflow:hidden; overscroll-behavior:none }` plus `.chat-messages-scroll, [data-scrollable] { overscroll-behavior: contain; -webkit-overflow-scrolling: touch }`. | `routes/globals.css` (~649, ~1470) | iOS Safari rubber-banded the whole document on every swipe instead of scrolling the chat/sidebar internally. The chat *list* separately carried no `data-scrollable` and so missed the contain rule. | `90e37113` "fix(mobile): CLI scroll, squish, premature reap"; list gap fixed in `b626223a` | commit-cited |
| C6 | Global `scroll-behavior: smooth` was removed; the three sites that want animation pass `behavior` explicitly. | `routes/globals.css` (absence); `mobile-tab-bar`, Talk `focusElement`, `use-stick-to-bottom` | Honest note: the commit states the removal claims **no measured win** - `html`/`body` never scroll and `scroll-behavior` is not inherited, so it was near-dead CSS. Removed because it would silently animate every programmatic scroll the day any surface scrolls the document. | `9aa551dd` "Drop the global smooth scroll-behavior" | commit-cited |

#### 2.4 The post-turn fold

| # | Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|---|
| D1 | Folding is per-frame **scroll-anchored** for 480ms, and the loop **yields** the moment it finds the scroller somewhere other than where it last left it - baseline seeded from the position the fold was *scheduled* from. | `components/chat/fold-anchor.ts` `anchorScrollDuring` | The fold happens above the answer the reader is reading; a naive collapse yanks the answer up. The browser's own `overflow-anchor` measured 146px off on a mid-viewport strip and Safari has none. Without the yield, a flick during the 480ms window was undone every frame ("writing on top of a fling is what kills the fling"), and a flick *before* the loop's first frame was overwritten. | `fold-region.tsx` header; `42a5bfd4` (yield); `cd069a89` (baseline from scheduling position) | commit-cited |
| D2 | The animated fold only plays when the scroller has enough slack (`canAnchorFold`); otherwise callers use an instant fold. | `fold-anchor.ts` `canAnchorFold` (+2px tolerance) | Compensation scrolls up by (region height − summary height) and `scrollTop` cannot go below 0; without the slack the animated path yanks content up by the remainder. | source comment | comment-only |
| D3 | A send **nominates** answered regions for collapse; each region **declines permanently** while any part of it is on screen. Only a region already scrolled off the top files itself away - and that one folds in a **single commit**, not an animation. | `fold-anchor.ts` `foldIsAboveViewport`; `fold-region.tsx`; test `__tests__/fold-send-stability.test.tsx` | A send used to collapse every answered region at once under the reader's eye. The single-commit path exists because a frame loop there has to win a race against everything else scrolling the transcript when a new ask lands, which it does not reliably do. | `8ab571b2`; enforced by `fold-send-stability.test.tsx` | commit-cited + test-enforced |
| D4 | Geometry that cannot answer (a detached node measuring all zeros) answers **no**. | `foldIsAboveViewport` | An unproven collapse is exactly the one that moves a pixel the reader was looking at. | source comment | comment-only |
| D5 | Region identity is `(anchorId, seq)` - **turn-scoped with a per-turn sequence**, never first-item-scoped. | `chat-messages.tsx` (~line 538) | A turn can hold several folds, and streaming grows a run's first evidence row; a first-item key remounted the region as a fresh instance resting *folded*, snap-collapsing the middle instead of playing the anchored fold. | source comment; region model from `641ac234` | comment-only |
| D6 | The fold's ledger summary line mounts **one commit after** the answer, compensated - never in the answer's own commit. | `fold-region.tsx` (~87) | Mounting it in the same commit pushes the answer down at the stream→final swap and breaks the structural-parity guarantee. | `8ab571b2` | commit-cited |
| D7 | A toggle cancels both the previous toggle's timer **and** its pending frame. | `fold-region.tsx` | A click cancelled the timer but not the frame, so an interrupted collapse still scheduled the timer that closed the region the reader had just asked to open. | `8ab571b2` | commit-cited |
| D8 | `finalAnswerIndices` scopes a fold to one **engine turn**, not one user turn: a child callback / agent relay closes the previous segment and opens a fresh fold. A segment that produced no prose keeps folding toward the downstream reply. | `chat-messages.tsx` `finalAnswerIndices` (~287-330) | The "Worked for" fold collapsed everything down to the turn's *last* block, swallowing an earlier visible reply and its fold. Tools-only segments orphaned as bare rows. | `641ac234` "resolve five post-v0.25 UI/session regressions" (bullet 1) | commit-cited |

#### 2.5 Markdown and message rendering

| # | Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|---|
| E1 | Engine scratchpad tags on their own line (`analysis`, `thinking`, `reasoning`, `reflection`, `scratchpad`, **`summary`**) are folded into a collapsed disclosure; a stray closing tag with no opener is **swallowed**, not printed. | `components/chat/message-markdown.tsx` `TRACE_TAGS` / `TraceBlock` | Issue #85: engine scratchpad tags leaked into the chat bubble as literal tag lines. `summary` was added separately because a leaked compaction message is an analysis block followed by a summary block - folding only the first half still dumped the whole recap into the chat. | `b0783896` (#85); `a7080cbb` "fold a leaked compaction summary, not just its analysis half" | commit-cited |
| E2 | An unterminated trace block runs **to the end of the message**, so mid-stream blocks fold too. | `message-markdown.tsx`; `streaming-format.ts` `traceCloseIndex` | Otherwise a still-streaming scratchpad block renders raw until its closing tag arrives. | source comments | comment-only |
| E3 | `formatMessage(content, { tightLines: true })` for **user** bubbles: a single Enter is a line break carried by line-height alone; a blank line is the 8px paragraph gap. Assistant markdown keeps the per-line margin. | `message-markdown.tsx`; test `__tests__/message-markdown-tight.test.tsx` | Both roles shared one formatter that gave every text line an 8px bottom margin - invisible inside wrapped assistant prose, but user messages are typed with single Enters so every line read as paragraph-spaced. | `507bd56e` "tight line breaks inside user bubbles"; test asserts exactly 3 vs 0 margins | commit-cited + test-enforced |
| E4 | The inline regex is compiled **fresh per call** (`new RegExp(...)` inside `inlineFormat`). | `message-markdown.tsx` `inlineFormat` | `inlineFormat` recurses for table cells; a shared `/g` regex's `lastIndex` leaks across the recursion. | source comment; regex consolidated in `bcf1eddc` | comment-only |
| E5 | The bare-file-path pattern and the anchored `FILE_PATH_RE` both derive from one shared `FILE_PATH_CORE` string. Backticked paths get a broader charset (viewer roots) than bare paths. | `components/chat/message-file-link.tsx` | The pattern was duplicated and drifting. The bare form is deliberately narrow so ordinary prose (mime types, branch names, version numbers, bare filenames, URLs) is not linkified - pinned by an 11-case table test. | `bcf1eddc` "file-path regex dedupe, viewer overflow fix, unit tests" | commit-cited + test-enforced |
| E6 | Markdown link hrefs are filtered through `safeMarkdownHref` (only `http(s):` / `mailto:`); a rejected link renders as **plain text**, not a dead anchor. | `message-markdown.tsx` | Blocks unsafe URL schemes reaching an anchor `href` in agent-authored content. | source code only; no commit body found | carried deliberately, reason unknown (mechanism obvious; no commit names the incident, no test enforces it) |
| E7 | Modified clicks (cmd/ctrl/shift/middle) on a file link **fall through** to the real browser route instead of the in-app tab. | `message-file-link.tsx` `FileLink` | Preserves open-in-new-tab. | source comment | comment-only |
| E8 | `MarkdownView` registers ~28 Prism grammars by hand instead of using the full `Prism` build. | `components/markdown-view.tsx` (top) | `PrismAsyncLight` ships no grammars; the full build is ~200 grammars / ~250KB gzip. Unregistered languages render unhighlighted as accepted degradation. Related: the PWA precache deliberately names chunks rather than a glob, because `out/assets` holds ~350 files, most of them per-language Prism chunks. | source comment; `1aded399` "Install the web UI as a PWA and cut its first load" | commit-cited |
| E9 | Markdown wrappers force `break-words` + `[overflow-wrap:anywhere]`, code blocks cap at `max-w-100%` with `whiteSpace: pre-wrap` via `codeTagProps`, scroll containers get `overflow-x-hidden`, content wrappers `min-w-0`. | `markdown-view.tsx`, `file-view.tsx` | The shared file/markdown viewer showed a horizontal scrollbar on long tokens; verified in-browser against a lockfile's sha512 hashes and a long-line `.tsx`. | `bcf1eddc` part C | commit-cited |
| E10 | GFM task lists are re-skinned: `list-style:none`, `::marker { content: "" }`, and an `appearance:none` custom round checkbox - including a **separate rule for `li.task-list-item`** because the custom `<ul>` renderer drops remark-gfm's `contains-task-list` class. | `routes/globals.css` (~1513-1557) | The default disabled `<input>` plus a leading disc bullet read as browser chrome; and the class the first rule keys on never survives the custom renderer. | source comments; `4ffad378` "redesign Notes to Apple Notes parity" | commit-cited |

#### 2.6 Streaming

| # | Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|---|
| F1 | The streaming bubble formats **incrementally**: settled line runs are formatted once and handed back as the *same* React elements; only the volatile tail is re-formatted. | `components/chat/streaming-format.ts` | `formatMessage` is O(buffer) and ran over the whole buffer on every token, so per-token cost grew through a long reply - main-thread work that leaves the compositor waiting mid-flick. | `42a5bfd4` | commit-cited |
| F2 | `stableLineCount` **mirrors `formatMessage`'s own loop** (fences, trace blocks, pipe lines) rather than approximating it, and never settles the last line or a lone trailing empty line. | `streaming-format.ts` | A split boundary inside a fence, a trace block or a table renders differently split than whole. `formatMessage('')` renders nothing while an empty line inside a longer buffer renders a spacer. A pipe line may still become a table header when its separator arrives. | source comments; module from `42a5bfd4` | commit-cited (module), comment-only (each rule) |
| F3 | A buffer that does not `startsWith` the settled text resets all settled state. | `streaming-format.ts` `format` | A rewritten/redacted stream is a different stream; without the reset the settled prefix would be prepended to unrelated content. | source comment | comment-only |
| F4 | The streaming container and the final `MessageRow` render through **one shared shell component** (`AssistantRowShell`), and `turnSpacerClass` is one function used by the streaming container, the pre-token Thinking indicator, and the final row. A test compares the two DOM signatures byte-for-byte. | `chat-messages.tsx` (~587), `components/chat/turn-spacer.ts`, test in `__tests__/comms-v2.test.tsx` (`shellSignature`) | The stream→final swap must be a pure text-node replacement - zero movement by construction, "no hand-copied class strings that can drift apart". | `4a98a36e` "comms v2 - … streaming parity"; test-enforced | commit-cited + test-enforced |
| F5 | A `session:delta` snapshot **replaces unconditionally** - no length gate. | `hooks/use-live-session.ts` (~728) | A shorter/rewritten snapshot (a redaction transform) must win; the old length gate left the pre-redaction streamed text visible for the rest of the turn. | source comment | comment-only |

#### 2.7 Optimistic sends and reconciliation

| # | Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|---|
| G1 | `reconcileMessages` matches on a **content-identity key** (role + content + media fingerprint by file *name*) as well as by id. | `lib/conversations.ts` `messageIdentityKey` | A v0.16.0 regression: the optimistic user message (client random id) and its server twin (canonical id) both rendered - the whole user message, text + image + video, appeared **twice**. The database was verified as holding exactly one row. Name-based fingerprint is stable across the optimistic base64-url copy and the server file copy. | `cfbcd3e2` "inbound user message with media no longer renders twice" | commit-cited |
| G2 | On a match, the server row's **content** is adopted but the **optimistic id and timestamp are kept**, and keep winning across later snapshots. | `lib/conversations.ts` `reconcileMessages` | The client-uuid → server-id swap changed the React key, remounting the user bubble and every turn/fold region anchored on its id - a visible flicker on the first send of every chat. | `641ac234` (bullet 2) | commit-cited |
| G3 | Local rows are queued **per identity key and consumed once** (`localByKey` array; `unsyncedRows` counts credits rather than testing membership). | `lib/conversations.ts` | Identity keys are content-only, so repeated identical messages ("ok", "yes") share a key. Without per-match consumption two server "yes" rows both adopt the same optimistic id → colliding React keys and turn anchors; and one older settled "yes" would stand in for every later "yes", silently swallowing a newer pending or failed one. | `641ac234`; `6d4bef70` "dedupe live session paged snapshots" | commit-cited |
| G4 | When nothing was re-keyed, the function returns the **snapshot array by reference**. | `reconcileMessages` (`rekeyed` flag) | Callers rely on `=== snapshot` to skip re-renders when the merge is a no-op. | source comment | comment-only |
| G5 | Preservation of unsynced rows is **age-capped at 5 minutes** (`RECONCILE_PRESERVE_MAX_AGE_MS`). | `lib/conversations.ts` | A message that failed to persist server-side was re-appended on every reconciliation forever. | `a47eb4dd` "age-cap preserved media messages in reconcile" | commit-cited |
| G6 | Send state is `pending \| failed` **on the message**, with "sent" being the *absence* of a value. Retry drops the failed row before re-sending. | `lib/conversations.ts` `sendState`; `components/chat/message-send-state.ts`; `send-failure-row.tsx` | A failed send appended a synthetic assistant "Error: …" row while the reader's own bubble still looked delivered - one event, two voices, and the wrong one carried the failure. Dropping the failed row on retry avoids a ghost beside the new attempt. | `85ad96f7` "Give a sent message an honest state and an entrance" | commit-cited |
| G7 | `--danger-fill` is defined in **four** palette blocks (`:root`/`[data-theme=dark]`, `[data-theme=light]`, and both `prefers-color-scheme` media blocks), at that block's own alpha - and a test asserts each block independently, plus that no rule hardcodes the colour. | `routes/globals.css`; test `components/chat/__tests__/send-motion-tokens.test.ts` | A token defined only in the `[data-theme]` pair is **missing for a reader who never picked a theme** and is on the OS preference. The test's `block()` helper walks braces so one palette's assertion cannot be satisfied by a neighbour's declaration. | `85ad96f7`; test-enforced | commit-cited + test-enforced |

#### 2.8 Live session lifecycle

| # | Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|---|
| H1 | `loadSession` must **never** set `loading = true`. Loading is owned by `handleSend` (true) and the socket's `session:completed`/`stopped` (false). | `hooks/use-live-session.ts` (~1082, ~1210) | A stale GET arriving after completion re-armed the spinner and stuck, because the completion event had already passed. Clearing it on session-change would clobber the lazy-init `loading = true` for a pane mounted with a `pendingUserMessage`. | `38adeaec` "make loadSession resilient to stale daemon responses"; `f98193cf` | commit-cited |
| H2 | Three layered recoveries for a dropped `session:completed` frame: a reconnect backfill (debounced 300ms), a post-reconnect **watchdog** on silence, and a poll while the UI believes a turn is running. | `use-live-session.ts` (~1232-1315), `COMPLETION_WATCHDOG_MS` | `session:completed` is a single point of failure: if that one frame dies with a half-open socket at completion, `loading` stays true forever. The poll exists because the reconnect-only watchdog never gets a chance when the socket itself never reconnects. | `fcda2f1e` "chat backfill on active turns + completion watchdog" | commit-cited |
| H3 | Backfill also triggers on the **local** `loadingRef`, not just `status === 'running'`. | `use-live-session.ts` (~1241) | `handleSend` sets `loading` without setting `status='running'` (status is only refreshed from the server later), so a reconnect mid-turn skipped the backfill and never recovered the deltas missed while the socket was dead. | `fcda2f1e` | commit-cited |
| H4 | An `interrupted` session is refetched on reconnect and its spinner cleared locally. | `use-live-session.ts` (~1047, ~1233) | A gateway restart marks the in-flight session "interrupted"; the event that would clear the spinner died with the old gateway, leaving the chat permanently unusable. | source comments; `fcda2f1e` | commit-cited |
| H5 | The turn-start marker is anchored **by message id and remapped inside the updater**, never held as an index. | `use-live-session.ts` (~1062) | The server snapshot may be ≤150 rows; a snapshot-space index truncated already-rendered history at turn completion. | source comment | comment-only |
| H6 | A zero `timestamp` is an explicit "unknown" sentinel and is **never** replaced with `Date.now()`. | `use-live-session.ts` (~281, ~1028) | Using `Date.now()` makes a legacy row appear to have happened at hydration time and fabricates elapsed work every time the transcript is reloaded. | source comment | comment-only |
| H7 | A cached resting snapshot may stand in for a revisit without a refetch, but **fails closed** for anything uncertain (running, unknown status, missing session), and lifecycle events drop the snapshot of any session that changes while unmounted. | `use-live-session.ts` (~231-254) | Back/forward hops across sessions re-issued a full transcript GET per hop; the fail-closed rule stops a stale snapshot standing in. | source comments | comment-only |
| H8 | The just-mounted child pane's meta emit is deferred a **microtask**. | `use-live-session.ts` (~1219) | Child effects flush before the parent page's ref-sync effects, so the emit was filed under the *previous* session. | source comment | comment-only |
| H9 | Live-completion animation sentinels are scoped to the session where the terminal event arrived and are **never inherited** by a restored snapshot. | `use-live-session.ts` (~1149) | A restored snapshot would replay another session's completion animation. | source comment | comment-only |

#### 2.9 Arrival motion

| # | Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|---|
| I1 | At most **3** rows per commit play an enter animation, and the cap counts **rendered** rows (`renderedMessageIds`), not raw messages. | `components/chat/message-arrival.ts` `LIVE_ARRIVAL_BATCH_MAX` | The cap counted raw messages, so a commit whose tail was a delegation's own tool calls (which the grouping drops) spent every slot on rows that never render, leaving the chip beside them to appear instantly. Uncapped, a large delivery becomes a multi-second stagger tail. | `8ab571b2` | commit-cited |
| I2 | Rows carrying their own block arrival (`delegation`, `dispatch`) are removed **before** the cap counts. | `message-arrival.ts` `hasOwnBlockArrival` | `use-live-session` mints a `LiveBlockArrival` for exactly those two types and `ChatBlockInline` plays it; a generic mark on top animated the row twice. | source comment; `8ab571b2` | commit-cited |
| I3 | An enter mark has a **1s TTL**. | `message-arrival.ts` `ENTER_MARK_TTL_MS` | Long enough for the slowest 260ms enter plus a paint; short enough that a windowed row the reader scrolls back to (which remounts) never replays an animation it already played. | source comment | comment-only |
| I4 | `prefers-reduced-motion` is read **live at commit time** (not via a subscribed hook) in the arrival path, and emits **no marks at all**. | `message-arrival.ts` `prefersReducedMotion`; also `fold-region.tsx` | The CSS flattens the animation but the stagger *delay* is ours and would still run, holding rows back invisibly. Reduced motion also emits no tool-group marks, so the stagger cannot run invisibly. | source comments; `ee09884d` | commit-cited |
| I5 | An assistant row that is all blocks carries the enter mark on the **bubble**, because there is no transcript element to carry it. | `chat-messages.tsx` (~597) | Those rows silently skipped their entrance. | `8ab571b2` | commit-cited |

#### 2.10 Media

| # | Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|---|
| J1 | An image renders into a box declared at its **own ratio** - server-measured `width`/`height` when present, else a ratio remembered **per URL** from a previous decode, else 4/3 - with `object-contain`; the skeleton and the broken-image tile fill the same box. | `components/chat/media-dimensions.ts`, `message-media.tsx` | A single image reserved a flat 140px then painted at natural height, so a portrait screenshot grew the transcript by ~450px **at decode time**, under a reader the open had already let go of. | `ee09884d` "Reserve the box a picture will need, and let tools arrive" | commit-cited |
| J2 | A decoded image reporting 0×0 yields `null`, not `NaN`. | `media-dimensions.ts` `toRatio` | A broken/empty image divides to NaN, a ratio that collapses every box it sizes. | source comment | comment-only |
| J3 | The content-growth ResizeObserver calls `pinToEnd` (through the virtualizer), **not** `pinNow` (raw `scrollHeight`). | `use-stick-to-bottom.ts` (content RO) | The observer exists for media decode; a raw `scrollHeight` write on a windowed transcript aims at the estimate and stops short - "wrong for a windowed transcript in two other places" per the commit. | `ee09884d` | commit-cited |
| J4 | The engine-only `Attached files:` block is stripped from every rendered bubble. | `lib/conversations.ts` `stripAttachedFilesBlock`, called once per `MessageRow` body | The gateway appends it to the prompt for the engine CLI; it must never be shown, since attachments render as chips/thumbnails. Now load-bearing in a second way - see the render-cost test in 2.11 L4. | `608ea57b`; comment marked "Defensive" in `chat-messages.tsx` | commit-cited |
| J5 | `isVideoMedia` treats MIME as authoritative. | `lib/conversations.ts` | Rows persisted before `video` existed as a media type are stored as `file`. | source comment | comment-only |

#### 2.11 Typography, fonts, CSS, and source gates

| # | Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|---|
| K1 | Mobile input zoom guard is `font-size: max(1rem, var(--text-body)) !important` - **not** a flat 16px. | `routes/globals.css` (~672-679) | iOS Safari zooms on focus below 16px, and the page ships `user-scalable=no`, so that zoom is one the reader **cannot undo**. The rule used to lean on the body step bottoming out at 1rem, which the new smaller Text-size steps made false. A flat 16px would stop the input following the setting above the floor. | `de112ff5` "Let the reader pick the UI's text size, per device" (says the old comment "said so and is corrected here") | commit-cited |
| K2 | `.notes-title` opts **out** of that guard with a higher-specificity `!important`. | `routes/globals.css` (~1504) | The guard pushed the note title down to the body step on mobile; the title is well over 16px and not a zoom risk. | source comment; `4ffad378` | commit-cited |
| K3 | An **em/en dash source gate**: a test walks every non-test `.tsx?` in `components/chat/` and `components/ui/`, strips block and line comments, skips lines containing `.match(` or `RegExp`, and fails on any em or en dash. | `components/chat/__tests__/comms-v2.test.tsx` ("rendered copy carries no em or en dashes") | Enforces the no-dash copy rule in rendered UI strings. Its own carried fix: it splits on `/\r?\n/` because a trailing `\r` survives a `\n` split and `.` never matches `\r`, so on a CRLF checkout the comment-stripper reached nothing and **every commented dash in the codebase reported as rendered copy**. | `4a98a36e`; test-enforced | test-enforced |
| K4 | Fonts are self-hosted woff2, split latin / latin-ext by `unicode-range`, weight-ranged `400 600` for the variable UI face and three discrete mono weights, `font-display: swap`, hashed into `/assets/`. | `routes/globals.css` (lines 1-95), `src/fonts/` | The two web-font provider links were render-blocking, third-party and guaranteed to miss offline. The `400..600` range was kept **identical to the previous request so nothing re-renders**. Files in `public/` would revalidate every load; hashed assets are served immutable. | `1aded399` | commit-cited |
| K5 | A build plugin injects a `<link rel=preload as=font crossorigin>` for **only** the latin variable UI face, and **throws the build** if that file is not in the bundle. | `packages/web/vite.config.ts` `preloadUiFont` | The UI face is only discoverable after the stylesheet downloads and parses - a full round trip behind the HTML. `crossorigin` is required because fonts are fetched in CORS mode even same-origin; without it the preload misses and the browser fetches twice. latin-ext and mono are conditional on what a page renders, so preloading them is a wasted request. | `1aded399` + source comments | commit-cited |
| K6 | A **blocking inline script** in `<head>` applies theme and `--text-scale` before first paint, duplicating the `TEXT_SCALES` list because an inline script cannot import it. | `packages/web/index.html` | Reading them from a React effect renders the app once at the default and reflows. | `de112ff5`; source comment | commit-cited |
| K7 | `viewport-fit=cover` is required for `env(safe-area-inset-*)` to resolve to anything but 0px, and all four insets are exposed as `--safe-*` tokens composed **at the call site** with `max()`. | `index.html`, `routes/globals.css` (~300) | Without `viewport-fit=cover` every `--safe-*` token silently resolves to 0. | source comments; `f08b31b0` / `8940db0f` for the token set | comment-only |
| K8 | `--tab-bar-height: 56px` exists as a token specifically because the value was written out twice and the copies disagreed. | `routes/globals.css` (~306) | One copy parked a third of the status bar's controls behind the tab bar. | source comment; `c68339ef` "ship the page-chrome contract" | commit-cited |
| K9 | `--chat-top-clearance: max(var(--safe-top), 12px)` replaced a `+52px` reservation. | `routes/globals.css` (~323) | The old value reserved space for a removed solid header and left a dead gap above the first message. | source comment | comment-only |
| K10 | The chat list's control band carries its **own** top safe-area inset, and a test asserts the inset value rather than reachability. | `chat-sidebar.tsx`; test `__tests__/chat-list-safe-area.test.tsx` | The chat route is `chromeless`, the branch that skips PageLayout's `pt-[var(--safe-top)]`, and the mobile thread header is hidden over the list. jsdom does no layout, so a reachability test **passes with the bug in place**. | test header; test-enforced | test-enforced |
| K11 | xterm: `.xterm-helper-textarea { pointer-events: none !important }`, `.xterm-screen { margin-inline: auto !important }`, `touch-action: pan-y !important` on `.xterm`, explicit `user-select: text`, plus a `touchmove → term.scrollLines()` handler and a byte-stream U+FE0E patch. | `routes/globals.css` (~1429-1468), `components/cli-terminal.tsx` (~175, ~350) | Clicking the terminal focused the hidden helper textarea and **stole focus from ChatInput**. iOS Safari ignores `scrollTop` on xterm's absolutely-positioned overflow viewport, so it reports a scroll but does not visually scroll. `.xterm-screen` snaps to whole cells (853px in ~870px), so the 5-20px leftover showed as dead background on the right only. TUI glyphs rendered as colour emoji on iOS; appending the zero-width text-presentation selector forces the text form, and works below Safari 17.4 where `font-variant-emoji` does not. | `90e37113` + source comments | commit-cited |
| K12 | The service worker: `/api` is **NetworkOnly** (stated explicitly "so a future default cannot quietly start caching it"), navigations are NetworkFirst with the cache key **pinned to `/index.html`**, `navigateFallback` is disabled, and `cleanupOutdatedCaches` is on. | `packages/web/vite.config.ts` | A cached Todos/sessions snapshot without a staleness marker is worse than an honest "gateway unreachable". `navigateFallback` means a deploy needs **two** reloads (the first still boots the old index). A per-URL navigation cache would make a route's offline availability depend on having visited it online. A superseded hashed chunk the user cannot clear is a permanent bug. | `1aded399` + source comments | commit-cited |

#### 2.12 Smaller carried behaviours worth porting

| # | Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|---|
| L1 | `stripMarkdown` strips only **paired, content-wrapping** emphasis, and leaves `__dunder__` intact entirely. | `lib/strip-markdown.ts` | It governs TTS input as well as preview labels, so blanket emphasis removal was a real voice-fidelity loss on identifiers, math and URL underscores. `__x__` is indistinguishable from a dunder identifier, so code fidelity wins. Fence lines strip any info string **before** the inline-backtick pass, so the delimiter and tag drop as a unit rather than leaving a stray language line. Newlines are preserved for downstream sentence splitting. | `7ad5a5f0` "stripMarkdown no longer mangles spoken code/math/URLs" | commit-cited |
| L2 | `useScrollAnchor` has **no dependency array**, and skips restoring when the previous snapshot was at `scrollTop === 0`. | `hooks/use-scroll-anchor.ts` | Any commit can be the one that changed a height above the reader. At the very top there is nothing to correct, and a snapshot taken before a programmatic scroll (the POP restore writes one on mount) would drag the container back to where it started. The caller **must** wire the returned handler to `onScroll` or a reader who scrolls between commits is corrected back. | source comments; `fb40f433` "Hold the reader's place when a Todo's status changes" | commit-cited |
| L3 | The prepend anchor may only be spent by a commit that changes `firstId`. The old rAF `.finally()` fallback was **deleted**. | `lib/scroll-anchor.ts` (`firstId`), transcript path | REPRO 1: with >400 messages, scroll up until the older page fires while a reply lands at the bottom → the message being read leaves the screen entirely (10000px in the fixture). The anchor was consumed by *any* change to `messages`; the reply's commit spent the correction and cleared it, so the real prepend arrived uncorrected and the rAF fallback found nothing to apply. | `0dde83d6` (REPRO 1) | commit-cited |
| L4 | `MessageRow` memoisation is protected by hoisting the `onRetry` callback out of the render, and a test counts row-body executions per streaming token via a mocked `stripAttachedFilesBlock`. | `chat-pane.tsx` / `chat-messages.tsx`; test `__tests__/chat-render-cost.test.tsx` | REPRO 3: every token re-rendered all 500 message rows because a fresh inline arrow identity broke every row's memo. The test uses `stripAttachedFilesBlock` as the counter specifically because `formatMessage` sits behind a `useMemo` on text and would under-count. | `0dde83d6` (REPRO 3); test-enforced | commit-cited + test-enforced |
| L5 | The pane's loading spinner is a **250ms threshold measured from when the reader started waiting**, handed over from the route-level fallback through two module-scope variables and a 100ms handoff window. | `components/chat/chat-hydration.tsx` | A cold direct open waits at the route fallback first; paying the threshold again after it disappears splits one continuous wait into two loading states with a blank beat between them. | source comments; enforced by `first-send-continuity` tests at both pane and route level ("Either one alone still blanks the chat") | test-enforced |
| L6 | `registerChatComposerControl` uses a **token symbol** for cleanup. | `components/chat/chat-composer-control.ts` | An old pane unmounting after its replacement mounted was unregistering the replacement. | source comment | comment-only |
| L7 | Composer STT: any real keystroke while a send is armed **disarms**; programmatic transcript fills bypass `onChange` and set provenance directly; an empty transcript on an armed send disarms rather than sending blank; and both the manual-stop and timeout-stop paths funnel through one `applyTranscript`. | `components/chat/chat-input.tsx` (~352-596), `armed-send.ts` | Prevents an auto-send firing under an operator who took over, a blank message being sent, and the two stop paths behaving differently. `resolveSendTap` / `resolveTranscriptLanding` are extracted as pure functions with their own tests. | source comments; `__tests__/armed-send.test.ts` | comment-only + test-enforced |
| L8 | Peek panel focus restore happens in the **provider's** effect, not in `close()`, and uses `preventScroll: true`. | `components/peek/peek-stack.tsx` | Focus cannot land inside an inert subtree and the sheet inerts the app root while open; React runs every effect cleanup before any effect body, so the parent provider's effect is the first place the inert is gone. `preventScroll` because the mention sits mid-transcript and the thread has to look exactly as it did. | source comments | comment-only |
| L9 | Thread-peek releases its transition lock immediately when entering and closing resolve to the same offscreen transform. | `components/chat/thread-peek.tsx` (~106-129) | CSS cannot emit `transitionend` when the two states are identical, so a lock waiting for it never released (Escape before the double-rAF enter). | source comments | comment-only |
| L10 | The sidebar's stall/elapsed clock is **one shared interval via an external store**, refreshed on subscribe, ticking only for rows that could stall; unread uses a **neutral grey** dot, never `--accent`. | `components/chat/session-signals.tsx` | The clock stops with the last listener so the cached value can be arbitrarily old when the sidebar remounts. `--accent` is user-set and may be red, which would read like an error. A stalled turn shown with the same blue spinner as a working one is why a 51-minute hang sat unnoticed. | source comments | comment-only |
| L11 | Todo mention previews are batched across one render pass (`queueMicrotask` flush, ≤100 ids per request, ≤500 cached) and refreshed **event-driven, never timed**. | `lib/todo-preview.ts` | A transcript can render dozens of mentions in one pass; one request per mention. The gateway route rejects more than 100 ids. | source comments | comment-only |
| L12 | Bare Todo ids are rewritten in a **rehype pass on the hast tree**, skipping `code`, `pre` and `a` subtrees, with a fresh regex per call. | `lib/markdown-todo-mentions.ts` | Only text nodes are split, and never where the id is already spoken for. The walk is recursive so it shares no regex `lastIndex`. Mentions are **off by default** in `MarkdownView` - "a skill, a note, a file and a workflow's output mean the string". | source comments; `3d8bd699` "render todo comments as markdown" | commit-cited |
| L13 | Hover glance is **not rendered at all** below 640px, rather than rendered-and-hidden. | `hooks/use-hover-glance.ts` | So a tap can never land on it. | source comment; `e79308c2` "Give a Todo mention a hover glance" | commit-cited |
| L14 | Non-critical chunk warming waits for `window.load` **plus** a fixed 2500ms. | `hooks/use-idle-mount.ts` | Idle time alone is not late enough; the work must land outside the first-paint / pre-interaction network waterfall that is measured against. | source comment | comment-only |
| L15 | Stale-chat dismissals are stored in localStorage inside try/catch, capped at 100. | `lib/stale-chat.ts` | Quota or disabled storage must not break chat. | source comment | comment-only |
| L16 | The chat CSS uses a **borderless** language (fill + shadow) - jump-to-latest, composer card, code-block chrome - because "the old shadow token did not exist". | `jump-to-latest.tsx`, `chat-input.tsx`, `code-block-chrome.tsx` | Recorded as a deliberate revision, not an accident. | `6b767264` (last bullet) | commit-cited |

#### 2.13 Unexplained carries - chat / view layer

Items with no reconstructable *reason*. Method: `git log -S` on the distinctive substring
against `packages/web`, falling back to `--all --grep`, then reading the introducing
commit body. These are the ones whose introducing commit says nothing about them.
**Port them verbatim; do not tidy them up.**

1. **`crypto.randomUUID` polyfill in `<head>`** - `packages/web/index.html`, first script tag. Introduced by `22cc05c3` (the Next→Vite refactor), whose body does not mention it; `git log -S "crypto.randomUUID" -- packages/web/index.html` returns only that commit. It is load-bearing: `crypto.randomUUID()` is the id of every optimistic user message (`chat-pane.tsx` ×4, `use-live-session.ts` ×3), and the whole reconcile-by-identity machinery (G1-G4) assumes those ids exist. Note that `components/talk/context/browser-instance.ts` guards the call defensively while the chat path does not - the chat path depends entirely on this polyfill. Reason for its existence is recorded nowhere in-repo.
2. **`closePartialMarkdown` closes fences, inline code and bold, but not single-`*` italic or `_`** - `components/chat/streaming-format.ts`. Its docstring claims it handles italic; the code does not. `git log -S "closePartialMarkdown"` reaches `654c65f1` (2026-03-08, no body). Whether the omission is deliberate (an unpaired `*` is common in prose and closing it would corrupt more than it fixes) or a drifted docstring is not recorded. Port the **code**, not the docstring.
3. **`max-w-[280px]` on markdown table `th`/`td`** - `components/chat/message-markdown.tsx` `TableBlock`. `git log -S` returns nothing (the string predates the current path or moved with a rename). No comment. Combined with `min-w-max` on the table and `overflow-x-auto [WebkitOverflowScrolling:touch]` on the wrapper this is clearly a tuned wide-table behaviour, but no failure is recorded.
4. **`TEXT_PRESENT_GLYPHS` Unicode ranges** - `components/cli-terminal.tsx` line 179. The *mechanism* is documented (U+FE0E forces text presentation) and `90e37113` cites the iOS symptom, but the specific three ranges chosen are not justified anywhere. Copy the ranges exactly; a tidier range changes what renders.
5. **Tuned constants with a stated rationale but no recorded measurement.** Copy as numbers, do not re-derive: `SETTLE_WINDOW_MS = 400` (`transcript-open.ts`), `ANCHOR_WINDOW_MS = 480` / `FOLD_SUMMARY_PX = 32` (`fold-anchor.ts`), `SETTLE_MS = 120` (`touch-scroll-phase.ts`), `ENTER_MARK_TTL_MS = 1000` and the 90ms stagger (`message-arrival.ts`), `USER_COLLAPSE_PX = 240` / `USER_COLLAPSE_SLACK = 40` (`collapsible-user-text.tsx`), `OLDER_LOAD_THRESHOLD_PX = 900` (`scroll-anchor.ts`), `OVERSCAN = 8` and the estimate table 140/56/44/72/56 (`transcript-virtualizer.ts`), `SPINNER_DELAY_MS = 250` / `HANDOFF_WINDOW_MS = 100` (`chat-hydration.tsx`), `MAX_RECORDING_MS = 30min` (`use-stt.ts`), `cleanPreview` cache `MAX = 200`. Each has a prose justification in-source; none has a cited experiment.
6. **`safeMarkdownHref` scheme allowlist** (E6) - mechanism self-evident, but no commit or test names the incident, and no test enforces it. If the port drops it, nothing fails.
7. **`EMOJI_POOL`** - `lib/emoji-pool.ts`. The comment claims each emoji is "chosen for uniqueness at small sizes (16-48px)". No commit records the selection process. `emojiForName` is a deterministic hash, so **changing the list order or contents re-assigns every employee's avatar**. Port the array byte-for-byte.

#### 2.14 One correction to a carried belief

Program notes refer to a woff2 **"matched families"** carried fix. What is actually
evidenced at this commit is narrower: the fonts are self-hosted, split by `unicode-range`
into latin/latin-ext, and the variable UI face declares the weight range `400 600`
**explicitly matched to the previous web-font-provider request "so nothing re-renders"**
(`1aded399`), with only the latin variable file preloaded and a build-time throw if it
goes missing (`vite.config.ts`). No commit, comment or test in `packages/web` uses the
phrase "matched families"; `git log --all --grep='matched famil'` returns nothing. If that
phrase names a fix elsewhere (the public site is a separate surface), it is not this one.
Carry K4 and K5 as written; do not assume a broader rule exists.
---

### Part II - build, bundling, platform, routing, state plumbing, non-chat surfaces

#### 2.15 The build → serve pipeline (reproduce this contract, not just the files)

**Four separate mechanisms** cooperate to stop a browser booting a stale bundle. Dropping any
one of them reintroduces a bug that was individually hunted down.

```
packages/web/  --vite build-->  packages/web/out/        (outDir is `out`, NOT `dist`)
                                      |
        +-----------------------------+------------------------------+
        | scripts/sync-web-dist.mjs                                  | packages/shell/scripts/prepare-frontend.mjs
        v                                                            v
packages/jinn/dist/web/   <- served live by the gateway        packages/shell/dist/web/  <- Tauri frontendDist
  copy-all-except-index -> verify every /assets/ ref             stage to dist/web.next -> rm dist/web -> rename
  resolves IN TARGET -> write .index-next-<pid>-<ts>.html        (full replace; the shell is not live while syncing)
  -> atomic rename over index.html -> prune files not in
  the fresh build -> re-assert
```

Gateway serving (`packages/jinn/src/gateway/server.ts` `serveStatic`, `request-handler.ts`):
- Route order is **CORS → OPTIONS → auth gate → `/api/` dispatch → static**, and that order is itself a security property (a 404-vs-200 on a plugin path is only visible to an authenticated caller).
- `/assets/*` → `public, max-age=31536000, immutable`; everything else → `no-cache`.
- A **missing** `/assets/*` file returns `404 text/plain`, never the SPA fallback.
- Any other missing path returns `index.html` (SPA fallback); `/` with no build returns a 503 "Web UI not built" page.

#### 2.16 Build, bundling and caching

| Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|
| Vite `build.outDir: 'out'`, not `dist` | `packages/web/vite.config.ts` | Legacy Next.js output path, kept "for gateway compatibility" - the sync script, perf-budget script, turbo `outputs`, and both consumers all hard-code `out`. | `22cc05c3` "replace Next.js with Vite + React Router" | commit-cited |
| `sync-web-dist.mjs` copies **everything except `index.html`** first, re-verifies every `/assets/` ref resolves *in the target*, writes `.index-next-<pid>-<ts>.html`, then `rename()`s it over `index.html` | `scripts/sync-web-dist.mjs` | The gateway serves this directory **while it is being written**. A naive recursive copy lets a client fetch a new `index.html` whose chunks have not landed yet → `Failed to fetch dynamically imported module` on a live instance. | `3d122e00` | commit-cited |
| `pruneStale()` deletes every target file not in the fresh build - and runs **after** the index swap, never before | same | Vite hashes chunk names, so the target only ever grew. A browser that revalidated `index.html` but still held an older entry chunk **kept booting a months-old bundle out of the same directory**. Pruning before the swap would break the copy-then-swap ordering. | `55aef723` "release: v0.28.4 - … prune stale web bundles"; source comment states both halves | commit-cited |
| Gateway 404s a missing `/assets/*` instead of SPA-falling-back to `index.html` | `packages/jinn/src/gateway/server.ts` `serveStatic` | The fallback returned HTML with `Content-Type: text/html` for a `.js` request, so the browser reported an unrecoverable-looking MIME error for a merely superseded chunk. | `a2438ce9` "fix(web): recover route chunks and cache chat switches"; test `gateway/__tests__/static-web-assets.test.ts` | commit-cited + test-enforced |
| `no-cache` for HTML, `max-age=31536000, immutable` for `/assets/*` | same | iOS Safari over a tunnel hostname caches HTML indefinitely and serves stale JS/CSS. | `90e37113` | commit-cited |
| `.webmanifest` in the gateway MIME map → `application/manifest+json` | `packages/jinn/src/gateway/static-mime.ts` | Served as `octet-stream`, the browser ignored the manifest and **the install prompt never appeared**; and without the extension the SPA fallback would hand back `index.html`. | `1aded399`; test-enforced | commit-cited + test-enforced |
| Brotli/gzip of hashed assets is memoised, keyed on `resolvedPath \0 mtimeMs \0 encoding`, LRU-bounded (256 entries / 32 MB); **non**-hashed assets stay on the streaming path | same | Recompressing every chunk on every request. The three-part key is what stops a rebuild, a second instance serving a same-named file from another directory, or a different `Accept-Encoding` from being answered with the wrong bytes - proven by four dedicated tests. | `d9eac83b`; test-enforced | test-enforced |
| `lazyRoute()` wraps every route import: recognises 6 chunk-failure message shapes (including the text/html MIME one), reloads **once** per `routeName + pathname` via a `sessionStorage` latch, memoises the import promise and clears the memo on rejection | `src/lib/lazy-route.ts` | The client half of the same bug. The one-shot latch is what stops a genuinely broken deploy becoming an **infinite reload loop**. `Route.prefetch` deliberately swallows - a real render retries. | `a2438ce9`; test `lib/__tests__/lazy-route.test.ts` | commit-cited + test-enforced |
| `preloadUiFont()` Vite plugin **throws the build** if the latin variable woff2 is not in the bundle, and emits the preload with `crossorigin=""` | `vite.config.ts` | Only the latin variable face is preloaded (preloading a face nothing draws is a wasted request). Fonts are fetched in CORS mode even same-origin; without `crossorigin` the preload misses and the browser fetches twice. The throw prevents a silent rename turning the plugin into a no-op. | `1aded399` | commit-cited |
| Workbox `globPatterns` names files explicitly instead of a `**/*.js` glob | `vite.config.ts` | `out/assets` holds ~350 files, most of them per-language Prism chunks behind a lazy route. A glob precaches megabytes nobody reads. | `1aded399` | commit-cited |
| `navigateFallback: undefined` plus a hand-written NetworkFirst navigation rule with `cacheKeyWillBeUsed: () => '/index.html'` | `vite.config.ts` | Workbox's default precache-first navigation means **a deploy needs two reloads to take effect**. Pinning the cache key stops a route's offline availability depending on having visited it online. | `1aded399` | commit-cited |
| `public/sw-shell-warm.js`, `importScripts`ed by the generated worker, fetches `/index.html` with `cache: 'no-store'` during `install` | `packages/web/public/sw-shell-warm.js` | A runtime cache is only written by navigations the worker intercepted - and the navigation that *installs* the worker is not one of them. Without this, **an app opened exactly once has no shell to fall back to offline**. | source comment (the whole file header) | comment-only |
| `/api/*` is an explicit `NetworkOnly` rule | `vite.config.ts` | Todos, sessions and the roster are a live ledger; a cached snapshot rendered without a staleness marker is worse than an honest "gateway unreachable". Stated explicitly so a future workbox default cannot quietly start caching it. | `1aded399` | commit-cited |
| `cleanupOutdatedCaches: true`, `skipWaiting`, `clientsClaim` | `vite.config.ts` | A superseded hashed chunk the user cannot clear is a permanent bug. | `1aded399` | commit-cited |
| `manualChunks` buckets react / react-router / tanstack-query - and **deliberately gives Radix and cmdk no bucket** | `vite.config.ts` | The `vendor-radix` bucket cost **11.7 KB gzip on first load**: one shared chunk meant a single primitive in the shell dragged in every primitive any route used. Dropped, Rollup files each with its route (184190 → 172441 B gzip, cause named). | `1aded399`; bucket introduced `26bd5cc5` | commit-cited |
| `@jinn/plugin-sdk` is a **Vite alias to a source file**, not a package | `vite.config.ts`, `tsconfig.json`, `vitest.config.ts` (all three must agree) | A real package would need its own build and its own React peer, and the singleton the SDK exists to guarantee is exactly what a second React copy would break. | `cc162bac`; test `plugins/sdk/__tests__/runtime.test.ts` requires the alias be named dynamically so a broken mapping fails the build | commit-cited + test-enforced |
| Three `@jinn/*-wire` aliases point **into `packages/jinn/src`**; `workflow-wire` is types-only and never resolves at build time, the other two are pure zero-import leaves that really ship | `vite.config.ts` | Keeps the editor judging a model id by the same rule the config loader uses. "A second copy of that rule is a second answer waiting to drift." The zero-import-leaf property is what makes importing gateway source into a browser bundle safe with no polyfills. | source comments | comment-only |
| `@swc/core` must be in `pnpm-workspace.yaml` `onlyBuiltDependencies` | `pnpm-workspace.yaml` | Global `ignore-scripts` hardening means no dep build script runs unless allowlisted, and `@vitejs/plugin-react-swc` needs its native binary. | inferred from the allowlist | carried deliberately, reason unknown |
| Turbo `lint` lists `packages/gateway-events/dist/index.d.ts` as an explicit **input** rather than adding `^build` | `turbo.json` | Without the emitted declarations, `act(() => emit(...))` in the web tests widens to a thenable and **eleven phantom `no-floating-promises` errors** appear. Listing it as an input keeps the lint cache honest without inflating the graph. | source comment | comment-only |
| CI runs `perf:budget` inside the `build` job, on that job's own `packages/web/out` | `.github/workflows/ci.yml` | `sync-web-dist.mjs` only clears its own target, so `out/` survives - reusing it avoids paying for a second full build. | source comment | comment-only |

#### 2.17 Platform and the native-shell boundary

The web bundle carries **no `@tauri-apps/*` and no `@capacitor/*` dependency at all.**
Everything native goes through a window-injected bridge. This is the single most important
thing for the port to preserve.

| Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|
| The native bridge is `window.__JINN_NATIVE__`, typed but never imported | `src/platform/native-bridge.ts` | "One bundle still serves the browser, the PWA and the shell" - the shell injects globals rather than the web taking a native dependency. The earlier Capacitor spike established the pattern; Tauri inherited it. Initial critical path grew by 218 bytes gzip. | `8978665f` | commit-cited |
| `createLazyTauriAdapter()` defers its dynamic import and short-circuits on `runtime.container !== "tauri"` **before** awaiting the loader | `src/platform/adapters/lazy-tauri.ts` | Keeps native-only code off the browser's initial path entirely; a browser session never even fetches the chunk. | `docs/platform.md` | commit-cited |
| Adapter chain resolves the **first adapter whose `capability()` reports `supported`**, with a bare `catch {}` per adapter | `src/platform/contracts.ts` `createPlatform` | Capability discovery is advisory and must never break a caller. | source comment | comment-only |
| `startKeyboardInset()` writes `--keyboard-inset` on the root from `visualViewport.height`/`offsetTop`; the token is also declared `0px` on bare `:root` | `src/platform/viewport.ts`, `adapters/web.ts`, `routes/globals.css` | The `:root` default is what makes the variable resolve **before the adapter runs** and in browsers with no `visualViewport`. The unsubscribe is deliberately dropped in `main.tsx` (document-lifetime). | source comments | comment-only |
| `viewport-fit=cover` in `index.html` | `packages/web/index.html` | What makes the existing `env(safe-area-inset-*)` tokens resolve to anything but 0px - without it every `--safe-*` token is 0 and the shell's chrome sits under the notch and home indicator. | `8978665f` | commit-cited |
| Inline `crypto.randomUUID` polyfill built on `getRandomValues`, before any app code | `packages/web/index.html` | `crypto.randomUUID()` is **secure-context only**. Over plain HTTP on a LAN or tunnel hostname it throws and **crashed the chat page entirely**. `getRandomValues` *is* available in non-secure contexts. | `90a7d779` "fix(web): polyfill crypto.randomUUID for non-secure contexts" | commit-cited |
| Blocking inline bootstrap reads theme + text scale from local storage and stamps `data-theme` / `--text-scale` before first paint; the step list is a hand copy of `TEXT_SCALES` | `packages/web/index.html`, mirrored in `src/lib/settings.ts` | Reading them from a React effect renders the app once at the default and reflows. A later regression: the settings provider *still* started at defaults and read storage in a mount effect, so a "Larger" device painted 1.25 → 1 → 1.25. Initial state now reads storage during first render. | `de112ff5`; `338bef1f` | commit-cited |
| `SECURE_CONTEXT_CAPABILITIES` gates share / clipboard / notifications / badges before any call | `src/platform/adapters/web.ts` | Same non-secure-context class of failure as `randomUUID`, handled as a capability answer rather than a throw. | test `platform/__tests__/contracts.test.ts` | test-enforced |
| Tauri shell CSP is `connect-src ipc: http://ipc.localhost` - **no network at all** from the web bundle in the native shell | `packages/shell/src-tauri/tauri.conf.json` | Forces every HTTP and WS call through the native bridge with base64 bodies. ⚠️ `script-src 'self'` does **not** include `blob:`, so the plugin runtime-loader's blob-URL import cannot run under this CSP as written. | source (the CSP string itself) | comment-only; the blob/CSP interaction is a surveyor observation, not a documented fix |
| Native gateway profile switch calls `queryClient.cancelQueries()` + `clear()` **and** wipes 5 named plus 3 prefixed storage keys before commit | `src/lib/native-gateway-bootstrap.ts` | Profile-bound UI state (chat tabs, read sessions, sidebar, note drafts, per-session view mode) leaking from one gateway into another. | acceptance criterion in merge `cae1c95b` | commit-cited |
| `ClientProviders` is keyed `gateway:${generation}` | `src/main.tsx` `AppShell` | A profile switch remounts the whole provider tree rather than leaving live subscriptions pointed at the old origin. | n/a | carried deliberately, reason unknown (mechanism clear; no commit narrates the failure) |
| Service worker registers **only** under `import.meta.env.PROD && !nativeBridge()` | `src/main.tsx` | In dev a worker would sit in front of the gateway proxy and serve yesterday's bundle; in the native shell it would sit in front of the IPC bridge. | source comment | comment-only |
| `packages/shell/scripts/test-native.mjs` skips the native test off macOS and **says so** rather than reporting a pass | `packages/shell/scripts/test-native.mjs` | The crate pulls a keyring with an Apple-native backend; say plainly it was skipped rather than reporting a pass nobody earned. | source comment | comment-only |

#### 2.18 Routing and app-shell wiring

| Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|
| `/todos` and `/kanban` redirect through a **route loader**, not an element-level `<Navigate>` | `src/main.tsx` `routeLoaders`, `routes/todos/board/todos-index-redirect.tsx` | An element redirect renders `null` for one full commit, unmounting the page into an empty root for a painted frame. On mobile the **tab bar visibly flashed out on every chat→todos tap**, and the tab links' view transition then animated *to that empty frame*, stretching the flash across the whole transition. Verified by DOM mutation trace. | `e314b481` | commit-cited |
| The plugin splat route `path: '*'` is registered **last**, and receives a computed `reserved` segment list | `src/main.tsx`, `routes/contributed-route.tsx` | A contribution can never shadow a core route. | source comment | comment-only |
| `APP_ROUTES` is a single frozen descriptor list; elements stay in `main.tsx` but identities live in `lib/app-routes.ts` | `src/lib/app-routes.ts` | Talk coverage and the router consume the same list instead of maintaining two uncheckable copies. `matchAppRoute` is first-concrete-match-wins with the splat last. | test `lib/__tests__/app-routes-talk-coverage.test.ts` | test-enforced |
| The chat route gets its **own** `<Suspense>` inside the shell's Suspense | `src/main.tsx` `routeElements` | The reader sees one loading state instead of "loading page" and then, a beat later, "loading chat". | source comment | comment-only |
| Route prefetch is registered per-href in a module-level Map; Chat and TodoBoard are warmed on `requestIdleCallback` with a `setTimeout(…, 0)` fallback | `src/lib/route-prefetch.ts`, `src/main.tsx` | Hover/idle prefetch without the router knowing about it. | `d9eac83b` | commit-cited |
| Talk and plugins navigate through **module-level handles** (`registerTalkNavigator`, `registerHostNavigator`) - Talk's returns the promise, the plugin's drops it | `src/main.tsx` | Both navigate from outside a render. The promise is what tells the Talk tool surface the route actually landed, "the only honest end for its latency clock". A plugin has no clock, so it drops it. | source comments | comment-only |
| `AppErrorBoundary` renders a "Web UI needs a refresh" button rather than a stack | `src/main.tsx` | Last-resort recovery for a chunk failure that got past `lazyRoute`'s one-shot reload. | `a2438ce9` | commit-cited |
| Provider nesting order in `ClientProviders` is load-bearing at three points | `src/routes/client-providers.tsx` | `TalkOrbOverlay` sits **above the router** so route changes never remount the orb; `PluginNotices` **before** `PluginHostBridge` so the sink is registered before a frame can route into it; `DiskPluginsBridge` **after** the host bridge because a plugin's module body may read host state the moment it evaluates. | source comments (all three) | comment-only |
| `MOBILE_TAB_ITEMS` derives from `NAV_ITEMS`, contributed rows always append last, and the static exports are the feature-disabled snapshot | `src/lib/nav.ts` | "The rail's order is the operator's mental model of their company, and a plugin does not get to insert itself into the middle of it." The disabled default keeps a newly installed or gated feature out of every static consumer before the runtime feature query resolves. | source comments | comment-only |
| `buildShortcuts()` **throws** on any id present in bindings but not the catalog, or vice versa | `src/lib/shortcut-catalog.ts` | A shortcut listed in Settings that does nothing, or a bound action nobody can discover, is a bug worth failing loudly for. | source comment | comment-only |

#### 2.19 State plumbing (React Query plus the event bus)

| Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|
| Global defaults: `refetchOnWindowFocus: true`, `refetchOnReconnect: true`, `refetchOnMount: **false**` | `src/lib/query-client.ts` | All three were `false` for perf, then focus/reconnect were **re-enabled** because a half-open socket left REST-backed queries stale after sleep. Mount refetch stays off to avoid churn on every remount - safe only because the sessions query merges (rather than trims) loaded pages on refetch. | `49eab1a6` then `d85842a6` | commit-cited |
| `TODO_QUERY_FRESHNESS = { staleTime: 10_000, refetchOnMount: true }` - a per-surface override of the global rule | `src/lib/query-keys.ts` | An invalidation that lands while the surface is unmounted must not survive its next mount. | `2c988bba` "fix(todos): resync board queries on return" | commit-cited |
| `TODO_CACHE_ROOTS` names **three** roots (`work-items`, `work-item`, `work-item-preview`) | `src/lib/todo-caches.ts` | React Query compares keys **element-wise**: `['work-item-preview']` is *not* prefix-matched by `['work-item']`. A write lane naming only the latter left the mention glance strip and the peek panel showing the pre-write value. | source comment | comment-only |
| `refetchTodoPreview()` calls `forgetTodoPreview(id)` **before** invalidating | `src/lib/todo-caches.ts` | The mention queryFn resolves out of a module-level promise map sitting *behind* React Query, so a refetch hands back the same stale promise unless the id is dropped there first. | source comment | comment-only |
| Event invalidation is debounced 1000ms with a 2000ms **max-wait**; the pending set is coarse keys | `src/hooks/use-query-invalidation.ts` | Debounce raised 500→1000ms; the max-wait stops a continuous event stream starving the flush forever. | `49eab1a6` | commit-cited |
| On flush, **only** the todo keys are deferred while a todo write is mutating, kept in the set, and retried next quiet window | same | A drag commit / editor save / approval decision holds an optimistic view; a refetch landing mid-flight clobbers it. Every other category flushes immediately. | source comment | comment-only |
| `company:changed` for a todo applies a **version-aware synchronous patch** *and* schedules the debounced pass | same, `handleCompanyChanged` | An older event can never overwrite a newer cached revision. The patch alone cannot insert a created Todo or prove it still belongs under the board's other filters. | source comment | comment-only |
| `session:background` is patch-only (no invalidation) - **except** when the session has a `parentSessionId` | same | These fire on every background-activity change including a clear; invalidating would be a refetch storm. A delegated child is the exception because its runtime state feeds transitive parent summaries. | source comment | comment-only |
| `session:deleted` calls `removeFromSessionsCache` *before* invalidating | same | Merge-on-refetch would otherwise keep it as a previously-loaded extra. | source comment | comment-only |
| Cron gets **two** invalidations (`['cron']` and `['cron-jobs']`) | same | The cron routes query a raw `['cron-jobs']` key that `['cron']` does not prefix-match. Same element-wise-matching trap as the Todo roots. | source comment | comment-only |
| `nextReconnectDelay()` uses **equal jitter** (`[window/2, window]`), not full jitter, and clamps the exponent at 31 | `src/lib/ws-backoff.ts` | Full jitter occasionally collapses to ~0, so the delay stops growing; the floor guarantees growth while still decorrelating a fleet reconnecting after one gateway restart. The clamp stops the exponent overflowing. | `d85842a6` | commit-cited |
| `captureVisibleAnchor` / `restoreVisibleAnchor` re-implement scroll anchoring in JS | `src/lib/scroll-anchor.ts` | Safari implements no `overflow-anchor`, so on a phone this cannot be left to the browser. Measuring the anchored row absorbs whatever height the change turned out to have; the `scrollHeight` delta is only the fallback when the row is no longer rendered. | source comment | comment-only |
| No client-side re-filter of server results for `q` / `since` / `until` / `label`; the due window is the **one** sanctioned client dimension | `src/lib/todos.ts` | A title-only client pass would silently discard body-only matches (a shipped bug, QA 2026-07-10). And the wire has no `due` param "and must not grow one". | source comment naming the shipped bug | comment-only |
| `publicWorkItemReference()` rejects any string containing a transport-only work-item id | `src/lib/todos.ts` | Transport-only ids must never cross into user-facing metadata. | source comment | comment-only |
| The client-side `attachment:` ref grammar is a deliberate **copy** of the gateway's, with a strict anchored regex | `src/lib/attachment-ref.ts` | A ref is an employee-authored string, so a lax parser here would let a path or a whitespace-smuggled token reach an image `src`. Kept as a copy, not a shared package, because the web bundle takes no gateway source. | source comment | comment-only |
| `transition-edges.json` is a checked-in **mirror** of the gateway's `transitions.ts` | `src/lib/transition-edges.json`, `lib/legal-targets.ts` | Client/server drift on board drag legality. The mirror carries its own comment naming the gateway parity suite that fails on drift. | `fb06f16a`; `packages/jinn/src/work-items/__tests__/board-legality-parity.test.ts` | commit-cited + test-enforced |

#### 2.20 Plugin loading (the no-build ESM door)

| Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|
| Import rewriting runs over `codeOnly(source)` - a comment-masked copy with byte offsets preserved | `src/plugins/runtime-loader.ts` | The import-specifier regex cannot tell code from a comment quoting the same syntax. Masking comments to spaces (offsets intact) lets the scan skip comments while the rewrite still edits real text. String/template state is tracked only so a `//` inside a URL literal is not read as a comment. | source comment | comment-only |
| The specifier regex is anchored on `from` / `import (` / `import `, is byte-identical to the engine's, and is exported so a test can pin it there | same, `importSpecifierRe` | "The anchor is the whole safety property, and widening it is how a rewrite starts reaching text that is not an import." A string literal containing a package name passes through unchanged. | source comment | comment-only |
| `shimUrl` uses `Object.hasOwn`, not truthiness | same | The import map is an ordinary object, so a prototype key answers with a function - **`import 'constructor'` would pass the allowlist**. | source comment | comment-only |
| `activate()` disposes the previous incarnation **before** the new one registers; a throwing disposer is caught per-disposer | same | Registering first would leave the old disposers holding entries the new registration has already replaced; one cleanup throwing must not strand the rest, or a reload leaves half the previous incarnation live. | source comments | comment-only |
| `installPluginSdk()` is called **inside** the try | same | It now fetches the SDK barrel's chunk: a deploy that superseded it should be reported against this plugin, not surface as an unhandled rejection in the reconcile pass. | source comment | comment-only |
| `diskPluginsSettled()` - a one-shot settle flag with listeners; a failed pass still settles | `src/plugins/disk-plugins.ts` | A deep link to a contributed page is rendered before any plugin has loaded, and a host that answered "no such route" in that window would bounce every plugin bookmark to chat. "We looked" is the fact the waiting side needs. | source comment | comment-only |
| The disk door maps **gateway id → loaded plugin id** as two separate values | same | The two differ the moment an edit changes the plugin's declared id, and telling them apart is the whole reason this map exists. | source comment | comment-only |
| esbuild compiles a plugin's `client.js` **only if the output contains a jsx-runtime import** | `packages/jinn/src/plugins/client-transform.ts` | JSX is a superset of ESM, so one parse answers both "does it compile?" and "was any of it JSX?". A plain-ESM file keeps its own bytes. Asking under the JS loader first would compile every JSX plugin twice. | source comment | comment-only |
| `jsxDev: false` is set explicitly | same | The dev runtime imports `react/jsx-dev-runtime`, which the loader's allowlist does not carry: every compiled plugin would fail to resolve. | source comment | comment-only |
| Compile **errors** are cached alongside successes, keyed on a file stamp | same | A plugin that will not compile must not be re-parsed on every load until its author fixes it. | source comment | comment-only |
| Plugin assets are served `Cache-Control: no-store`; unknown extensions 404, not 403 | `packages/jinn/src/gateway/plugins-api.ts` | Plugin files hot-reload under a stable name, so no response here may be cached - the year-long immutable policy static assets get is exactly wrong for them. 404-not-403 so the response does not confirm the file exists. | source comments | comment-only |
| The router mounts under `/api/plugins/` specifically | same, `PLUGIN_ROUTE_PREFIX` | Everything below `/api/` reaches the gateway's auth gate; a top-level route would not, and would be served to anyone who can reach the port. Exported so the auth-namespace test asserts against the real prefix. | source comment | test-enforced |

#### 2.21 Non-chat surfaces - Todos and Workflows

| Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|
| `@xyflow/react/dist/style.css` imported **per canvas surface** (3 files) | `routes/workflow/editor/editor.tsx:1`, `routes/workflow/run-canvas.tsx:1`, `components/org/org-map.tsx:1` | The global import was deleted so React Flow CSS ships only with a canvas chunk. The editor route was added *after* and shipped with an unstyled canvas until the import was re-added. **Any 4th canvas surface must repeat it.** | `21fd1b9f`; `e175b5a2` | commit-cited |
| The inspector picks desktop-rail vs mobile-sheet with a JS `matchMedia` hook, never CSS hiding | `routes/workflow/editor/inspector.tsx:699-741` | CSS mounted **both** shells. The editor store has no draft layer, so two mounted bodies meant two live controls writing the same node, plus duplicate labels and testids. | `e175b5a2` | commit-cited |
| Mobile sheets anchor at `bottom: calc(49px + var(--safe-bottom))` | `routes/workflow/editor/inspector.tsx:725`, `palette.tsx:89` | The sheets rendered *behind* the mobile tab bar. 49px is the tab bar's tap-target row. | `e175b5a2` (subject only) | commit-cited (subject only) |
| Stored node coordinates are honoured only if `ui.layout === "manual"` **and** every node has a position | `routes/workflow/editor/graph.ts:88-95`, `editor/store.ts:68-74` | Agents author definitions through the API and write a fixed 200px snake grid - **narrower than the 224px card** - so cards overlapped and edges were untraceable. The completeness half stops an agent-appended node landing at the origin. | `3980256a`; `__tests__/layout.test.ts` | commit-cited + test-enforced |
| `nodeBox()` is hard-coded per type and re-spread on every mutation; never DOM-measured | `routes/workflow/editor/ports.ts:56-68`, `store.ts:59-65` | dagre and `freeCenter` do collision math off this box; a measured box desyncs from the rendered card (condition cards grow one 30px row per case). **Adding a Condition case must re-spread `nodeBox`.** | source comment | comment-only |
| `outputPorts("workflow-call")` derives `exhausted` from `config.iterate`, and `success` **must stay index 0** | `routes/workflow/editor/ports.ts:96-104`; consumed at `store.ts:150, :190-196` | A static port list meant an iterating call grew no `exhausted` handle, so opening a saved looping workflow and touching the node **silently deleted the exhausted wire**. Reordering would make insert-on-edge wire new nodes to `exhausted`. | `34873e10` | commit-cited |
| `useOutputPorts` fires `updateNodeInternals()` keyed on a NUL-joined **port-id** string | `routes/workflow/editor/ports.ts:70-80` | React Flow caches handle bounds per node; a handle added mid-life has no bounds, so edges attach at the wrong point. Keyed on ids, not count. The separator is an escaped `"\0"` - a raw NUL once made git treat the file as binary (`188502d1`). | `34873e10` | commit-cited |
| `serializeDefinition` and `replaceNode` **spread** the node rather than listing fields | `routes/workflow/editor/graph.ts:114-116`, `store.ts:47-50, 186-190` | The editor has no control for `mutex`. A field-listing serializer would drop it on an unrelated edit - **taking the guard off a running Workflow with nobody touching it.** | `__tests__/editor-mutex.test.ts` | test-enforced |
| Lifecycle writes cache a "burned" revision and re-GET before the next write | `routes/workflow/lifecycle-menu.tsx:113-126` | After a 409 the prop only catches up when invalidated queries return; the operator's immediate retry races that refetch and **repeats the refused write forever**. | `__tests__/lifecycle-menu.test.tsx` | test-enforced |
| Lifecycle menu and cron delete menu both wrap in a `display:contents` span that `stopPropagation`s pointerdown and click | `routes/workflow/lifecycle-menu.tsx:207-208`; `routes/cron/delete-menu.tsx:104-127` | Radix portals the menu out of the row, but **React bubbles synthetic events up the *component* tree, not the DOM tree** - so clicking Cancel in the archive dialog navigated into the workflow or opened the cron job. `contents` keeps the wrapper out of the row's flex layout. | source comments + both `__tests__` | test-enforced |
| `edgeTaken` is a hand-maintained **client mirror** of the gateway's `edgeActivated` | `routes/workflow/run-canvas.tsx:18-38` | The mirror predated `exhausted`, so an exhausted loop was painted as having taken `success` - **the canvas lied about which branch ran.** Divergence is silent; nothing cross-checks. | `34873e10`; `__tests__/run-iteration.test.tsx` | commit-cited |
| `<PendingDecision key={approval.nodeId}>` | `routes/workflow/approval-decision.tsx:139` | Without a key the next gate inherits a choice it never offered - the typed reason and radio selection carry over when one gate replaces another in place. | source comment | comment-only |
| The Todo list declares the virtual block's top offset as `scrollMargin` and subtracts it back off every row transform | `routes/todos/list/list-virtualizer.ts:19-31, 90-101`; `list-window.tsx:96-108` | The virtualizer compares a row's `start` (from the block top) against raw `scrollTop` (from the scrollport top). Undeclared, those are two coordinate systems off by the container padding, so **every row near the viewport top read as "above the reader" and took a visible scroll correction, killing momentum scrolling on iOS.** | `b626223a`; `__tests__/list-virtualization.test.tsx` | commit-cited + test-enforced |
| `getItemKey` reads through a **ref** with empty deps; row keys are group-scoped | `routes/todos/list/list-virtualizer.ts:60-64, 91-97` | The extractor's *identity* invalidates the whole measurement pass - a fresh closure per render rebuilds every measurement on every poll. Group-scoping stops a Todo hoisted into "Needs you" colliding with itself. | `d0777796` | commit-cited |
| The scroll element is copied into React **state** via an effect | `routes/todos/list/list-window.tsx:88-92` | The scrollport is an *ancestor*, so React attaches its ref after this subtree's layout effects; the virtualizer's first look finds null and - nothing else re-rendering - **the list stays permanently empty.** | `d0777796` | commit-cited |
| The windowing threshold counts only item rows, not flattened rows | `routes/todos/list/todo-list.tsx:69-77` | 40 Todos flatten to *exactly* 50 rows, pushing a short list onto the windowed path and breaking pre-existing scroll-anchoring tests. | `8f0a6323` | commit-cited |
| **The board is deliberately NOT virtualised** | `routes/todos/board/**` (absence) | `use-board-drag.ts` builds drop indices from the live rects of **every mounted card** and indexes the full item list with them. Unmounting off-screen cards lands drops on the wrong rank or status - "corruption, not jank." | `d0777796` (states it explicitly) | commit-cited |
| Board cards get `content-visibility:auto` + `contain-intrinsic-size:137px` on desktop, but `visible`/`auto` below 700px | `routes/todos/board/card.tsx:156-158` | `contain-intrinsic-size: auto` **revises each card's remembered height mid-scroll** (observed 8129→6112); on the phone an estimated height moved `scrollHeight` 2721→3981 mid-commit and scroll anchoring chased the phantom to an edge. | `334228d5` (measured numbers in the message) | commit-cited |
| The column FLIP measures `card.offsetTop`, explicitly not `getBoundingClientRect()` | `routes/todos/board/column.tsx:17-46` | A viewport rect also moves when the reader scrolls, so any re-render after a scroll read the scroll offset as movement and **slid all 53 cards in from a screen away**. | `334228d5`; `__tests__/board-page.test.tsx` stubs `offsetTop` | commit-cited + test-enforced |
| `hasStopLead()` never reads the clock; `stopLeadKey()` emits **one bit per element**, not one for the pair | `routes/todos/board/stop-cause.tsx:20-32` | A key that changed on every countdown tick would make the column re-measure and re-animate once a minute forever. Folding park and hint into one bit told the FLIP a chip and a chip-plus-hint were the same height, so the card below jumped. | `27881ee9` | commit-cited |
| Park countdown wakes at `min(next minute, expiry)` clamped to `[250ms, 30s]`; **no timer at all** when nothing is parked | `routes/todos/board/stop-cause.tsx:40-51` | A fixed 30s cadence left an expired park still visibly counting down - precisely the dishonesty the chip exists to remove. | `27881ee9` | commit-cited |
| Optimistic status rollback is **version-fenced**; success banks the confirmed revision into every cache | `routes/todos/todo-status-mutation.ts:63-93, 160-167` | Two rapid writes: the second confirms at v5 while the first is in flight; the first's failure rolls back to a v4 snapshot the Todo has genuinely left, resurrecting the pre-write status. | `__tests__/todo-status-mutation.test.tsx` | test-enforced |
| Optimistic sub-task adds use an in-flight **counter** and remove only their own pending child | `routes/todos/task-page/use-subtask-mutations.ts:41-46, 130-146` | Overlapping creates shared one snapshot: a refusal restored the tree from before *that* create, taking a sibling's still-in-flight row with it. | `318e5b0a`; `__tests__/subtask-optimistic.test.tsx` | commit-cited + test-enforced |
| Comments fetched as a **head window** `[0, total − tailLength)`, merged with the detail payload's embedded tail, deduped by **id**, with `skipToken` when nothing is missing | `routes/todos/task-page/comment-window.ts` | The detail response already embeds the last 10 comments; the page then re-fetched the first 500. Measured **284KB → 162KB (−42.9%)**. `skipToken` rather than seeded `initialData` so a short thread fires *no* request and a seed cannot freeze at first paint and outlive a delete. Dedupe by id because a comment landing between requests shifts rows, not ids. | `3508d405`; `__tests__/comment-window.test.ts` | commit-cited + test-enforced |
| The rich-text body editor refuses to be the editor when its own parse→serialize round-trip is not byte-faithful, falling back to a raw markdown textarea; `lastCommitted` is seeded from the serializer, never the raw body | `routes/todos/task-page/live-body-editor.tsx:22-33, 89-100, 118-133` | Agent-authored bodies arrive over the API. The editor's starter kit has no table node, and checked task items, literal HTML, setext headings, `*` bullets and `1)` all normalize - **a focus plus blur with zero edits silently rewrote regions the operator never touched.** | `ae738cad`; `__tests__/task-editor.test.tsx` | commit-cited |
| Board drag hovering an **illegal** column `break`s out of the geometry loop rather than `continue`ing | `routes/todos/board/use-board-drag.ts:172-183` | `continue` lets the pointer fall *through* a dimmed column into a legal neighbour's hit region and drop there. The dimmed column **is** the affordance; `break` makes it a dead zone. Paired with a rAF re-measure after lift because a legal-but-empty exception column materialises on lift. | source comments; `__tests__/board-drag.test.tsx` | comment-only |
| Touch drag: more than 5px of movement **before** the 300ms hold cancels the gesture; `pointermove` is `{ passive: false }` | `routes/todos/board/use-board-drag.ts:186-196, 232` | Without the cancel every attempted column scroll on a phone became a card lift. `passive:false` is what makes `preventDefault()` actually suppress native scrolling once lifted. Selection feedback fires on lift because that silent wait is what the OS uses a haptic for. | source comments | comment-only |
| Board columns use `keepPreviousData` **and** a second in-scope gate for items/loading/error | `routes/todos/board/use-board.ts:100-127, 156-172` | An out-of-filter column keeps its last page, so a status filter still rendered backlog cards; and a disabled query stays pending **forever**, pinning the whole board to its skeleton. | source comments; `__tests__/board-status-scope.test.tsx` | comment-only |
| Pickers listen for Escape and outside-pointerdown on `document` in **capture** phase | `routes/todos/pickers/picker-shell.tsx:28-42, 65-72` | The mobile sheet shell takes no focus, so the key never enters the picker's subtree. Capture also makes the innermost surface the one Escape closes. | source comment | comment-only |
| `PickerPopover` bails out of flip/clamp entirely when the measured height is 0 | `routes/todos/pickers/picker-shell.tsx:104` | jsdom lays nothing out, so the clamp computes a bogus shift and destroys the offsets the picker tests assert. | source comment | comment-only |
| `PickerInline`'s autofocus effect has **no dependency array**, self-latched with a ref | `routes/todos/pickers/picker-shell.tsx:190-199` | The status picker opens showing "Checking sub-tasks…" and grows real rows only when the close-gate tree query lands; a mount-keyed effect finds no row and never focuses. | source comment | comment-only |
| `useCloseGate` **reports** a failed child-count read; never defaults to 0 | `routes/todos/pickers/use-todo-quick-pickers.tsx:69-86` | Defaulting to zero offers a "Done" the gateway will refuse, and then blames the write for a read that never landed. | source comment | comment-only |
| The Todos search box accumulates a keystroke **burst** in a ref and re-seeds the overlay with the whole string | `routes/todos/search-launcher.tsx:20-42` | The overlay is lazy-loaded; keys struck before it mounts land on the button. Seeding with only the newest key meant typing "ab" quickly opened the palette reading "b". | `31f3f05f` | commit-cited |
| Board card row 3: label chips are `min-w-0` and truncating; the roll-up button keeps intrinsic width | `routes/todos/board/card.tsx:207-245` | Everything was `flex-none`; a third label in a 240px column pushed the roll-up past the clipped edge - **present in the DOM, 0px wide, unclickable** - and it is the only route into the sub-task tray from the board. | `fa6ad1c0` (measured 0 → 44px) | commit-cited |
| Blocked/escalated reason commits on **submit only**, never on blur | `routes/todos/task-page/banner.tsx:159-162` | A blur is not a decision - switching tabs mid-sentence would freeze a half-written reason onto the permanent record. | source comment | comment-only |
| Query error → retry page; only a null payload → "doesn't exist" | `routes/todos/task-page/task-page.tsx:259-289` | A transport or server failure is retryable and must never masquerade as deletion. | source comment | comment-only |

#### 2.22 Non-chat surfaces - settings, notes, cron, limits, experiments, org, and UI/shell primitives

| Quirk | Where | What it fixed | Evidence | Confidence |
|---|---|---|---|---|
| `scaffoldBottomPadding()` builds its `calc()` via `terms.join(" + ")` - the whitespace is the point | `components/shell/page-scaffold.tsx:39-56` | `calc()` only reads `+` as an operator when whitespace-delimited. Unspaced, the declaration is **silently dropped**, `padding-bottom` resolves to 0, and the last rows of every list sit under the tab bar and FAB. No console error. | `78a90af1`; test asserts no unspaced `+` across three flag combos | test-enforced |
| The collapsing header renders as `display: contents` | `components/shell/large-title-header.tsx:96-107` | A sticky box cannot escape its containing block; the header's block ended just under the subtitle, so past that distance **the title bar scrolled away instead of taking over**. `display:contents` re-parents the bar's containing block to the scrollport. | `78a90af1` | commit-cited |
| The sticky bar pins at a **negative** inset and cancels then re-applies the gutter with negative `margin-inline` | `routes/globals.css:1010-1020` | A sticky inset counts from the *content* box, so `top:0` pins the bar below the scrollport padding and leaves a band of unblurred content sliding past above it. The negative margin makes the material span the page while the text stays on the column. | `78a90af1` | commit-cited |
| The large-title collapse is **CSS scroll-timeline only** | `routes/globals.css:1026-1042`, enforced across `components/shell/**` | Stops anyone "fixing" the collapse with a scroll handler, which reintroduces per-frame React work on the scrollport shared with the chat and todos virtualizers. | `shell-contract.test.ts` greps the shell dir for scroll listeners, IntersectionObserver and collapse state | test-enforced |
| Reduced-motion and the no-scroll-timeline `@supports` case each get their **own** hard-coded fallback | `routes/globals.css:1044-1058` | A scroll-driven animation is not time-driven, so **neither** the duration tokens **nor** the global reduced-motion duration reset reaches it. | source comment | comment-only |
| `--tab-bar-height: 56px` exists as a token; the scaffold and FAB may not spell `55px`/`56px` | `routes/globals.css:306-312`; `primary-action.tsx:7-9`; `page-scaffold.tsx:48-52` | The 56px (0.5px edge + 6px padding + 49px row) was written out by hand twice and **the two copies disagreed - one parked a third of the status bar's controls behind the tab bar.** | `c68339ef`; both tests assert the literals are absent | test-enforced |
| The title bar is a 3-column grid whose **leading** track is allowed to collapse - a long title drifts ~39px left of centre | `components/shell/large-title-header.tsx:44-58` | True centre needs the trailing-control width mirrored. Without the collapse a long title runs *underneath* the trailing controls instead of truncating against them. | source comment (carries the measurement) | comment-only |
| **No `touch-action` on `html`** - the absence is documented so nobody re-adds `manipulation` | `routes/globals.css:665-673` | `touch-action: manipulation` on `html` **takes inertial scrolling away from every scroller below it in WebKit**: a flick on any long list stopped dead on finger-lift. It had been added for the 350ms double-tap delay, which the viewport meta already removes. Three independent bug reports. | `b626223a` | commit-cited |
| The page is locked (`height:100%; overflow:hidden; overscroll-behavior:none`) and `[data-scrollable]` contains | `routes/globals.css:648-655, 1470-1478` | Without the page lock iOS Safari rubber-bands the whole *document*; without `contain`, hitting a list edge chains the scroll to the page. This is why `PageScaffold`'s scrollport must carry `data-scrollable`. | `90e37113` | commit-cited |
| Mobile inputs get `font-size: max(1rem, var(--text-body)) !important` | `routes/globals.css:669-677` | iOS Safari zooms on input focus below 16px. The rule used to lean on the body step bottoming out at 1rem, **which the Text Size setting made false** at smaller steps - and the page ships `user-scalable=no`, so the zoom is one the reader cannot undo. | `de112ff5` | commit-cited |
| `--text-scale: 1` declared on bare `:root`, **outside** the Tailwind `@theme` block | `routes/globals.css:328-334` | An inline style on the root element only wins the cascade against a `:root` declaration; inside `@theme` it would lose to the theme layer. | `de112ff5`; `settings/__tests__/text-scale-persistence.test.tsx` | test-enforced |
| Every Radix overlay uses hand-written `animate-pop-in`/`animate-overlay-in`; overlay keyframes may not touch `transform` | `components/ui/{dialog,dropdown-menu,select,context-menu}.tsx` + globals keyframes | Two bugs at once: the shadcn `animate-in`/`zoom-in-95`/`slide-in-from-*` classes come from a package **that is not a dependency here** - they emitted *nothing*, so dialogs, dropdowns and selects had **no motion at all** while a duration class timed an animation that did not exist. And a keyframe writing `transform` overwrites the Tailwind translate that centres a dialog, **throwing it into a corner mid-animation**. | `518afb08`; `components/ui/__tests__/motion-tokens.test.ts` | test-enforced |
| Reduced motion **collapses the duration scale** rather than setting `animation: none` | reduced-motion `:root` block | `animation: none` leaves a View Transition with nothing to end, so **its snapshot freezes over the live page.** | `motion-tokens.test.ts` asserts the string is absent | test-enforced |
| Settings has one 600ms debounce, one queue, and `drain()` re-runs itself in `.finally()` | `routes/settings/use-config-commit.ts` | Per-field debounces race each other to the same document with no Save button left to reconcile them. Root incident: the config GET served keys the PUT refused, so one hand-added config key made **every** save fail - reported ~900 lines above the control that caused it, reading as a setting that quietly reverted. | `6fde20f1`; `settings/__tests__/instant-apply.test.tsx` | test-enforced |
| On a config conflict the queued edit is **discarded**, no retry; adopting a revision clears the timer *and* the pending document | `routes/settings/use-config-commit.ts:76-82, 116-127` | Retrying *is* exactly the clobber being refused. A queued edit built on the pre-reload document, sent after the adopt, would **carry the fresh revision past the staleness check and overwrite the edit the reload went to fetch.** Unmount inside the debounce window flushes, since there is no Save button to give the drop away. | `6fde20f1`; `settings/__tests__/config-conflict.test.tsx` | test-enforced |
| An emptied fallback model map is written as `null`, never `{}`; legacy migration writes `null` rather than omitting | `routes/settings/engines/model-map-model.ts:57-66`, `chain-model.ts:126-135` | The gateway **deep-merges** a config PUT and keeps every omitted key. `{}` merges to no change, so a cleared map survives on disk. Only explicit `null` deletes. | `d6f8d9c8`, `048fa713` | commit-cited |
| `mapPairProblem()` asks whether the id is spellable **before** consulting the substitute engine | `routes/settings/engines/model-map-model.ts:74-100` | Ordered the other way, a pasted tab-separated composite came back as "a target engine does not serve" - pointing at the engine when the fault is a control character in the row. **That misdiagnosis cost an hour in a live incident.** | `e968f19d` | commit-cited |
| Each emoji save is tagged with an incrementing id; only the newest may roll back | `routes/settings/emoji-rows.tsx:29-44` | Every save captured the value it replaced and restored it unconditionally on failure, so **a stale rejection undid a pick the gateway had already accepted** and then claimed the setting was unchanged. | `8b492a4e` | commit-cited |
| The optimistic plugin toggle skips rows whose status is `error` | `routes/settings/plugins/inventory.ts:86-94` | Enabling a broken plugin does not fix it; showing it as loaded for a moment would say it did. | source comment | comment-only |
| The note editor reinitializes **only on path identity change**, and refuses a new revision while a protected draft exists | `routes/notes/note-editor.tsx:112-151` | A changed revision for the same open path is an external cache refresh - reinitializing on it **replaces the operator's live unsaved draft with the server copy mid-typing.** | source comment | comment-only |
| Notes restores the last-open folder/note only when the navigation type is not PUSH, once per mount | `routes/notes/page.tsx:78-103` | Restoring on PUSH means **tapping the Notes tab teleports you into the last note** instead of the folders home. | `4ffad378` | comment-only |
| Limits classifies freshness at **display** time from `refreshedAt` vs a ticking clock, never from the server's `stale` flag; a degraded 200 keeps prior windows and their original timestamp | `routes/limits/use-engine-limits.ts:42-113, 209-216` | Server-computed staleness freezes when fetched, so a long-open tab showed "Updated 2m ago" for 19h-old data. Separately, a provider-error snapshot with no windows **wiped a previously-good 42% reading.** | `0e79ac36`, `c20d8719` (both ship blockers) | commit-cited |
| Every Limits refresh is `AbortController`-bounded at 8s with a settled latch; the in-flight guard releases on the timeout path | `routes/limits/use-engine-limits.ts:145-179` | The fetch helper had no timeout and the guard cleared only in `finally`, so **one never-resolving request permanently wedged the page** - timer, visibility, reconnect and manual triggers all discarded thereafter. | `c20d8719` | commit-cited |
| The lightbox closes on the *completed* click, and only if the pointerdown also landed on the backdrop | `components/ui/image-lightbox.tsx:56, 144-145` | Closing on pointerdown unmounted the overlay before release, so the browser delivered the matching click to whatever was now under the cursor - **usually the next thumbnail, which opened its own lightbox.** | `b11cba0d`; two named tests | test-enforced |
| Pointer capture calls are wrapped in bare try/catch | `components/ui/image-lightbox.tsx:165-169, 248-252` | Synthetic and jsdom pointers have no capture target and throw; the real-world twin is capture already gone after an interrupted touch. | source comments | comment-only |
| `VideoPlayer.sourceWith()` parses against a throwaway base and reassembles `pathname+search+hash`; short-circuits `blob:`/`data:` | `components/ui/video-player.tsx:12-29, 37` | `new URL(src)` throws on the relative file paths the app uses; returning the absolute form would turn every attachment src absolute. `blob:` sources are optimistic local previews before upload. | `fbee03d1`; test asserts `blob:` survives verbatim | test-enforced |
| `autoPlay` is gated on there being no restore point, paired with a metadata-load restore | `components/ui/video-player.tsx:41-54, 71-73` | Switching quality swaps the source and resets the element: without the gate a **paused** video starts playing; without the restore it restarts from 0. | `0019eecf`; two named tests | test-enforced |
| The cron delete mutation keeps the dialog open until the invalidation **resolves** | `routes/cron/delete-menu.tsx:90-102` | Closing on the response would flash the row back, because the list refetch has not landed. | source comment | comment-only |
| `ReadingChart` returns null below 2 readings; pads a degenerate range; falls back to index spacing when first and last timestamps are equal | `routes/experiments/reading-chart.tsx:18-37` | Each guard stops a division by zero producing NaN in the polyline points - for the ordinary cases of one reading, a flat metric, or several readings in the same millisecond. | `experiments/__tests__/page.test.tsx` | test-enforced |

#### 2.23 Tests that enforce a rule - carry these or lose the rule

These are gates, not unit tests. Several **read the source tree** rather than rendering
anything, and two fire **outside vitest entirely**.

| Rule enforced | Where | What it prevents | Confidence |
|---|---|---|---|
| No product module may read a native global or capability API (Tauri/Capacitor globals and packages, `navigator.share/vibrate/clipboard/setAppBadge`, `navigator.userAgent`, `Notification.*`, display-mode matchMedia) - everything through `src/platform/` | `src/platform/__tests__/product-boundary.test.ts` | Runtime detection scattering out of `platform/runtime.ts`, making the adapter chain unfalsifiable and dragging native-only code onto the browser's initial path. | test-enforced + doc-cited |
| Gateway origin, browser navigation, and direct `/api/` fetches may live only in `src/lib/gateway-transport.ts` (also bans a legacy gateway-URL env var in `vite.config.ts`) | `src/lib/__tests__/gateway-transport-boundary.test.ts` | A profile-aware transport any module can bypass by reading `location.origin` - which is exactly what breaks remote and native gateway profiles. | test-enforced |
| Three page-chrome rules over `src/routes/**`: no hand-rolled large title, no page-level accent button, no new bottom sheet outside a 6-entry known list; suppression only via `// jinn-shell: ok <reason>` | `src/components/shell/__tests__/shell-contract.test.ts` | Chrome re-implemented per route. The gate **self-tests its own detector**: it proves it survives prettier wrapping a class list over 5 lines, counts two sheets on one line as two, and that a hatch does not cover a sibling. | test-enforced |
| The hand-authored SDK `.d.ts` files must stay in two-way sync with `index.ts`; no app-internal specifier; contract version pinned; the icon-name union equals the icon map's keys; every v1.1.0 export still exists | `src/plugins/sdk/__tests__/sdk-contract.test.ts` | A renamed export breaking every installed plugin at load; an added icon typechecking in the plugin but rendering nothing. A derived `.d.ts` would inline internal import paths into the public API and turn a rename into a silent break. | commit-cited |
| Every SDK host verb must pass through the permission gate (the test denies one verb at a time and asserts each throws) | `src/plugins/sdk/__tests__/host-permissions.test.ts` | v1 grants everything, so without this the gate is decorative - and a door that was never narrow cannot be narrowed afterwards without breaking every plugin at once. | commit-cited |
| Motion is token-only across 11 named files; 8 dead animation classes banned; no raw `\d+ms` or `cubic-bezier(`; every animate token points at a real keyframe; overlays have equal enter/exit counts; nav links carry `viewTransition` | `src/components/ui/__tests__/motion-tokens.test.ts` | See 2.22 - the classes emitted nothing at all. | commit-cited |
| `--danger-fill` must exist in **all four** palette blocks | `src/components/chat/__tests__/send-motion-tokens.test.ts` | A token defined only in the `[data-theme]` pair is missing for a reader who never picked a theme and is on the OS preference. | commit-cited |
| No em or en dash in rendered copy (`components/chat/*`, `components/ui/*`) | `src/components/chat/__tests__/comms-v2.test.tsx` | House rule made mechanical. Its stripper splits on `/\r?\n/` because on a CRLF checkout every commented dash in the codebase reports as rendered copy. | commit-cited |
| No colour literal in `src/contrib/*` or `status-bar.tsx` | `src/contrib/__tests__/slot.test.tsx` | Colour cannot be resolved at runtime in jsdom, so the only place this invariant is visible is the source. Deliberately scoped: it is not a rule for the rest of the tree. | comment-only |
| `dangerouslySetInnerHTML` forbidden anywhere in the global-search overlay | `src/components/global-search/__tests__/match-snippet.test.tsx` | The gateway returns snippets as server-made highlight markup **over operator text** - the injection path. Snippets are parsed into nodes instead. | commit-cited |
| Two first-paint bans: the plugin host bridge may not statically import the SDK barrel; `client-providers.tsx` may not pull the Talk screen-context graph | `plugins/sdk/__tests__/plugin-host-bridge.test.tsx`, `routes/client-providers.test.tsx` | A measured **16 KB first-paint overage** that turned main red on the bundle budget. | commit-cited |
| Every `APP_ROUTES` entry needs Talk semantic context or a typed explicit gap; no duplicate ids; the coverage doc must start with the freshly rendered table | `src/lib/__tests__/app-routes-talk-coverage.test.ts` | A new route silently leaving the Talk orb guessing about a screen it cannot read; a hand-edited doc drifting from the router. | comment-only |
| Real app startup (`main.tsx` module body plus mount) must raise **no permission prompt**; the fallback adapter must return unsupported for every family, never throw; the four result kinds stay distinct; the Tauri adapter must not load outside Tauri | `src/platform/__tests__/contracts.test.ts` | A cold-start notification or clipboard prompt on first paint, and native adapter code entering the browser bundle. A prompt raised anywhere on that path - not only inside the platform - fails this. | commit-cited |
| The gateway event union is locked at **typecheck** time via `@ts-expect-error` on a misspelled event and a wrongly-typed payload | `src/lib/__tests__/gateway-events.types.test.ts` | Silent re-widening back to stringly-typed frames. ⚠️ **Fires in `pnpm typecheck`, not vitest** - a port that only runs the suite loses it. | comment-only |
| Every CLI-keybar escape sequence must be in the backend's raw-key allowlist | `src/components/__tests__/cli-keybar.test.tsx` | Otherwise the keypress is silently dropped at the socket boundary - a keybar button that visibly does nothing. Cross-package mirror of the PTY socket's allowlist. | comment-only |
| Full client status-transition legality matrix pinned status by status | `src/lib/__tests__/legal-targets.test.ts` | Client/server drift on board drag legality; both sides read `transition-edges.json`. A port taking only the web half loses one side of the mirror. | commit-cited |
| `layout.test.ts` re-declares its clearance constant locally rather than importing it | `routes/workflow/__tests__/layout.test.ts:7-9` | Pinned here rather than imported so loosening the layout constant cannot quietly loosen the test. | test-enforced |
| **Bundle budget plus forbidden-module gate**: per-chunk gzip ceilings, initial critical path ≤ 195,000 B (baseline 189,999 - ~5 KB headroom), each pattern must match exactly one asset, the task page may contain no rich-text-editor modules, **every** emitted `.js` scanned for native markers | `packages/web/scripts/perf-budget.mjs` + `perf-budgets.json`; CI `build` job | The editor being statically re-imported into the Todo task page after being made lazy; native shell code leaking into the web bundle. ⚠️ **Fires in the CI build job, not `pnpm test`.** | commit-cited |
| Render-cost budget, chat: a committed append executes a bounded number of row bodies; a streaming token executes **zero** | `src/components/chat/__tests__/chat-render-cost.test.tsx` | Before this, every row took the whole message array and every append cost 500. | commit-cited |
| Render-cost budget, Todos: an unrelated page-state change executes **zero** list-row and board-card bodies; the test first proves its own counter still counts | `src/routes/todos/__tests__/todo-render-cost.test.tsx` | A long list mounting every row of every group. Each assertion was watched go red with its memo reverted. Also records the deliberate non-rule that the board is not virtualised. | commit-cited |
| `pnpm test` for web is **`node ../../scripts/vitest-flaky-retry.mjs`**, not `vitest`: in CI only, a failed *file* is rerun once in a fresh process and reported FLAKY; a repeated failure stays red; fail-closed on an unreadable report | `scripts/vitest-flaky-retry.mjs` + `vitest-flaky-report.mjs` | "An agent that reruns a suite until it goes green and reports success is hiding a bug." Retry is at **file** level because a 3-file probe showed per-test retry cannot reach the failure modes: an assertion failure names its test, a hook throw marks the file SKIPPED, a collection throw yields nothing. Same mode that let ~180 tests go missing from a green Windows run. Filters are forward-slashed because a Windows report name would match nothing and silently rerun the whole suite. | commit-cited |
| Machine-wide cap of **2 concurrent vitest suites**, held across the retry, via pid-holding lock files; an env var overrides; never blocks in CI | `scripts/test-slot-gate.mjs` | Four build worktrees running their verify phase at once put a 10-core machine at load ~60; every suite crawls into timeout territory and the live gateway starves with them. | commit-cited |
| `testTimeout: 20_000` and a 5000ms async-utils timeout | `vitest.config.ts`, `src/test/setup.ts` | The web suite's async budgets measured the machine rather than the app; both are raised so the gate goes red on code rather than on load. | commit-cited |
| Vitest aliases must mirror `vite.config.ts` exactly | `vitest.config.ts` | A suite that resolved the SDK by a different route would prove nothing about what the app ships. | comment-only |

**jsdom compensation.** `src/test/setup.ts` is only 72 lines - most compensation is
**per-test**, so carrying setup.ts alone is not enough. Counts across web test files:
`scrollTo` 264, `matchMedia` 67, `ResizeObserver` 37, `getBoundingClientRect` 32,
`requestAnimationFrame` 23, `getContext` 17, `scrollIntoView` 11, `randomUUID` 6,
`IntersectionObserver` 2, `checkVisibility` 1, `play` 1. The two load-bearing ones:

- **`installVirtualLayout()`** (`src/test/virtual-layout.ts`) - a full opt-in jsdom layout engine (spies `offsetHeight` and `getBoundingClientRect`, redefines `clientHeight`/`scrollHeight`/`scrollTop` with browser clamping, replaces `scrollTo`). The virtualizer sizes the scrollport and every row from `offsetHeight`, which jsdom reports as 0 - and a zero-height scrollport renders **no rows at all**, so without this a windowed list comes out empty rather than windowed. Row rects derive from the transform the virtualizer itself wrote. **Must be installed before render.**
- **`ResizeObserver` + `DOMMatrixReadOnly` mocks** in setup, installed **only if absent** - React Flow needs measurement APIs jsdom lacks; the conditional install is load-bearing because one chat DOM test captures its own `ResizeObserver`. `DOMMatrixReadOnly` models **only `m22`**, which is why zoom-dependent canvas tests are fragile.

**Also: zero snapshot files in `packages/web`** - no `.snap`, no `__snapshots__`, no
`toMatchSnapshot`. Snapshot contracts are simply not part of this package's rule surface.

**Repo-root gates that also gate `packages/web`:** `pnpm ratchet --check` (300-line cap,
`size-baseline.json` with **153 web entries**, budgets may only shrink); `pnpm lint` with
`eslint-baseline.json` (**140 web entries**) as the only suppression channel - inline
`eslint-disable` is ignored outright; `scripts/check-footguns.mjs --diff <merge-base>` judging
only **added** lines (needs full clone depth).

#### 2.24 Quirks the harness may not inherit

A Rust plugin serving a pinned, content-addressed bundle changes the ground under the caching
half. One clause each.

**Gone or moot:**
- **`sync-web-dist.mjs`'s copy-then-swap** - moot if the bundle is published as one immutable content-addressed artifact rather than written file-by-file into a live-served directory.
- **`pruneStale()`** - moot for the same reason: a content-addressed artifact cannot accumulate a previous build's chunks.
- **`outDir: 'out'`** - a pure naming legacy; the harness may name it anything, but four consumers hard-code it, so change all four or none.
- **`emptyOutDir` and turbo `outputs`** - build-orchestration detail, not a runtime contract.

**Still required, in a different shape:**
- **`Cache-Control: immutable` for hashed assets, `no-cache` for the document** - still required. A content-addressed *bundle* does not make the *document* safe to cache; the iOS-Safari-over-a-tunnel failure was about the HTML, not the chunks.
- **404 (not SPA fallback) for a missing `/assets/*`** - still required as long as any client can hold a superseded entry chunk. If the new host pins one bundle version per session and never serves a mixed set, this weakens; **unknown whether it does.**
- **`lazyRoute`'s recovery plus one-shot reload latch** - still required on any host where a deploy can land between the document and a lazy chunk. Cheap; carry it regardless.
- **SPA fallback for unknown paths plus a reserved-segment list for the plugin splat** - required by the router shape, independent of host.
- **`application/manifest+json` MIME** - required for PWA install; a Rust static server needs its own MIME map entry, and this is an easy silent loss.
- **Service worker** - required only if the harness still wants installable PWA plus offline shell. If it does, the whole workbox config carries over verbatim, including the `/api` NetworkOnly rule and the pinned index cache key.
- **Compression memoisation keyed on path+mtime+encoding** - a Rust host will have its own compression story; the *rule* that survives is "never answer one encoding's bytes for another, and never serve a stale build's bytes after a rebuild."
- **`preloadUiFont` build-throw** - Vite-plugin-specific, but the underlying rule (preload the one latin variable face, with `crossorigin`, and fail loudly if it vanishes) is host-independent.
- **`perf-budget.mjs` native-marker scan** - worth carrying: it is the only mechanical thing stopping native shell packages entering the *web* bundle, and the shell/web split survives the port.

**Unknown:**
- Whether the harness plugin serves `/api/*` from the same origin as the bundle. If not, the whole `gateway-transport.ts` boundary test's premise changes, and the CORS same-origin reflection has no analogue.
- Whether the harness reproduces the **route order** (auth gate before `/api/` dispatch before static). That order is a stated security property, not an implementation detail.
- Whether the Tauri shell path survives at all. If it does, the CSP and the base64 native transport are non-negotiable, and the plugin loader's blob-URL import is a live open question under `script-src 'self'`.

#### 2.25 Unexplained carries - build / platform / surfaces

Deliberate, non-obvious, and the reason could **not** be reconstructed. In each case the
surveyor ran `git log -S` on a distinctive substring, `git blame`, read the introducing
commit's full body, checked the enclosing merge, and searched `__tests__` and `docs/`.
**No rationale was invented.**

1. **`perf-budget.mjs` writes its native markers as split literals** (`["@tauri", "-apps/"].join("")` and friends). The obvious reading is "so the gate does not trip on itself", but **the code does not support it**: the script reads only build output, never its own source, and nothing else scans `scripts/` for these markers. Introducing commit `e57f43dd` has a one-line body. **Carry the split literals verbatim; the reason is unrecovered.**
2. **`perf-budgets.json`'s asymmetric budgets** - the task page gets 85,000 B against an 11,740 B baseline (7x headroom) while the board page gets 25,000 B against 23,437 B (1.07x). `d9eac83b`'s body is empty. The forbidden-modules clause is explained by the diff; the numbers are not explained anywhere.
3. **`ClientProviders key={gateway:${generation}}`** - the mechanism is obvious (remount on profile switch) but no commit or test states what leaked without it.
4. **`@swc/core` in `onlyBuiltDependencies`** - inferred from the plugin, not narrated anywhere.
5. **`setup.ts`'s `typeof localStorage.clear !== "function"` guard** - implies some environment supplied a *partial* Storage. `84bcaa48`'s body is empty. Not recoverable.
6. **`ResizeObserver` / `DOMMatrixReadOnly` mocks** - the *what* is documented in-file; `2478ab83`'s body is empty, so *when it first bit* is unknown.
7. **`product-boundary.test.ts` / `contracts.test.ts` / `gateway-transport-boundary.test.ts`** - all three added by commits with **empty bodies** (`bffbf332`, `b9cd29e7`). Rules unambiguous; originating arguments recoverable only from `docs/platform.md` and the reconciling merge `cae1c95b`. The legacy-env-var clause is clearly a specific scar, but nothing states what went wrong.
8. **`app-routes-talk-coverage.test.ts`** and **`routes/notes/__tests__/navigation.test.tsx`** - `af1a8eaa` and `e74a3ee0` are both subject-only. In particular, why Notes needed a *source-grep* gate rather than a behavioural one is unknown.
9. **`TOOL_BUDGET_MS = 300`** (`components/talk/tools/budget.ts`) - the measurement *method* is documented; the number is asserted nowhere.
10. **Two different phone breakpoints on sibling surfaces** - the workflow inspector uses 767px, the Todo board uses 700px throughout. `git log -S '767px'` hits only bulk commits with no mention.
11. **The workflow sheets' bottom offset omits the `max(var(--safe-bottom), 6px)` floor** every other consumer applies. `e175b5a2` has a subject and no body. Cannot tell whether the missing 6px is deliberate.
12. **Autosave dirtiness ignores `select` and `dimensions` node changes** (`routes/workflow/editor/store.ts:83-97`). Introduced whole in `7fddbf86` with no rationale, no comment, no covering test.
13. **`flushNow`'s 50ms busy-wait poll** (`routes/workflow/editor/editor.tsx:75-79`). Why a poll rather than awaiting the queued promise is not recorded; no test.
14. **Board drag hit-box slop** (`col.rect.top - 60`, `Math.max(col.rect.bottom, col.rect.top + 200)`, `use-board-drag.ts:176`). No comment; only the 2600-line board commit `ae645a9c`.
15. **`freeCenter`'s 200-iteration guard cap** (`routes/workflow/editor/layout.ts:29`). `188502d1` explains the nudging, not the cap; what happens at 200 is unhandled and untested.
16. **`useChainDrag`'s 5px lift threshold and 300ms touch hold**, plus the travel-cancels-lift rule (`routes/settings/engines/use-chain-drag.ts`). Almost certainly tuned against a real scroll-vs-drag conflict, but nothing pins them - **a rewrite changing either number would not go red.**
17. **`ImageLightbox` duplicates its centring override as both Tailwind classes and an inline style** (`image-lightbox.tsx:147-148`). `da6d2a4e`'s body is empty; no test asserts position.
18. **`ReadingChart`'s `preserveAspectRatio="none"` plus `vectorEffect="non-scaling-stroke"`** - coherent, but no comment, no test, and `57bfa3f7` does not mention the chart.
19. **`EmojiPicker` closes on `mousedown` rather than `click`** (`components/ui/emoji-picker.tsx:40-48`). A classic ordering fix, but the release commit `940dce61` bundles unrelated work and says nothing.
20. **`components/ui/tooltip.tsx` wraps every Tooltip in its own `TooltipProvider`** - the comment states intent, not the failure; no test; unusual enough (it defeats a shared open-delay) that bug-fix vs preference could not be confirmed.

**Known-stale artefact, worth fixing during the port:** `run-canvas.tsx`'s "keep in step with
`runner.ts`" pointer is wrong - `edgeActivated` now lives at
`packages/jinn/src/workflows/run-graph.ts:45`. The mirror is real and load-bearing; only the
file pointer rotted.
## 3. Coupling report

Scope: `packages/web` at the pinned sha, cross-checked against the gateway daemon source in
`packages/jinn/src/gateway/`. Source survey only; no gateway was contacted.

**Headline.** The web client is *not* a REST consumer that happens to point at this gateway.
It is compiled against the gateway's source tree (three Vite path aliases resolve into
`../jinn/src/`), it re-implements two of the gateway's own algorithms as maintained mirrors,
and its two largest features (chat transcript, Todos) use the wire types as their state model
down to leaf components. Cosmetic coupling is the minority.

**C** = cosmetically coupled (repoint the URL and it works). **S** = structurally coupled
(shape, semantics, or ordering is baked into the UI).

### 3.1 Endpoint inventory

Compiled from client call sites **and** the server route registrations, so it is complete
rather than inferred from one side.

#### Auth
| Endpoint | Called from | What the UI does | C/S | Note |
|---|---|---|---|---|
| `GET /api/auth/state` | `lib/auth.ts:80`, `lib/native-gateway-profiles.ts:204` | Gates the entire app behind `AuthGate` | **S** | Four-field contract `{authRequired, authenticated, canBootstrapLocal, networkExposed, instance}`; the pairing wall is a state machine over it |
| `POST /api/auth/bootstrap` | `lib/auth.ts:96` | Exchanges a URL-fragment grant for a cookie | **S** | Sends a bootstrap-grant header; loopback-only server-side |
| `POST /api/auth/pair` | `lib/auth.ts:106` | Redeems a code or token | C | |
| `POST /api/auth/pairing-codes` | `lib/auth.ts:118` | Mints a code for a second device | C | Server refuses bearer callers here |
| `GET /api/auth/devices` | `lib/auth.ts:129` | Settings device list | C | |
| `DELETE /api/auth/devices/:id` | `lib/auth.ts:136` | Unpair; self-unpair re-runs the gate | C | |
| `POST /api/auth/logout` | `lib/auth.ts:143` | Clears cookie, re-checks state | C | |

#### Sessions / chat
| Endpoint | Called from | What the UI does | C/S | Note |
|---|---|---|---|---|
| `GET /api/sessions` | `lib/api.ts:757` | Sidebar; **merge-on-refetch** cache | **S** | Returns `{sessions, counts, perGroup}` - rows are *untyped passthrough*; group keys (employee name, `direct`, `cron`) are gateway vocabulary the sidebar switches on (`components/chat/chat-sidebar.tsx:1145-1179`) |
| `GET /api/sessions?group=&offset=&limit=` | `lib/api.ts:776` | "Load more" per group | **S** | Paging is per-group, not global |
| `GET /api/sessions?q=` / `?pinned=1` | `lib/api.ts:759,779` | Search / pinned rail | C | |
| `GET /api/sessions/:id?last=&messages=` | `lib/api.ts:783` | Transcript load plus watchdog probe | **S** | `messages=0` is a status-only probe the completion watchdog depends on |
| `GET /api/sessions/:id/messages?before=&limit=` | `lib/api.ts:793` | Older-history prepend | **S** | `{messages, hasOlder}` cursor contract |
| `GET /api/sessions/:id/children` | `lib/api.ts:796` | Delegation tree | C | |
| `GET /api/sessions/:id/transcript` | `lib/api.ts:853` | Raw JSONL view | **S** | `TranscriptEntry` mirrors engine block shape |
| `PUT /api/sessions/:id` | `lib/api.ts:798` | Rename / mid-chat model swap | C | |
| `POST /api/sessions` | `lib/api.ts:807` | New chat | C | Body is untyped |
| `POST /api/sessions/:id/message` | `lib/api.ts:809` | Send turn | **S** | The optimistic bubble settles on the *first socket frame*, not this response |
| `POST /api/sessions/:id/stop` and `/reset` | `lib/api.ts:811,813` | Stop / unstick | **S** | Stop optimistically patches `status:'interrupted'` (`hooks/use-sessions.ts:87`) |
| `POST /api/sessions/:id/{archive,unarchive,duplicate}` | `lib/api.ts:799-803` | Row actions | C | |
| `DELETE /api/sessions/:id`, `POST /api/sessions/bulk-delete` | `lib/api.ts:801,805` | Delete | **S** | Delete must beat merge-on-refetch, hence an explicit cache eviction |
| `GET/POST /api/pins`, `DELETE /api/pins/:key` | `lib/api.ts:766-768` | Pin rail | C | |
| `GET /api/sessions/:id/queue` plus 6 mutations | `lib/api.ts:845-851` | Queue drawer | C | `QueueItem` is one level in |

#### Work items (Todos) - the largest surface
| Endpoint | Called from | What the UI does | C/S | Note |
|---|---|---|---|---|
| `GET /api/work-items?…` | `lib/api.ts:881` | Board columns | **S** | The board issues **one request per display status** because the gateway caps `limit` at 20 |
| `GET /api/work-items?ids=` | `lib/api.ts:980` | Mention-preview batch | **S** | 100-id cap encoded client-side (`lib/todo-preview.ts:10`) |
| `GET /api/search/work-items` | `lib/api.ts:907` | Filter-bar search | C | Same payload as list |
| `GET /api/work-items/:id` | `lib/api.ts:975` | Task detail | **S** | `WorkItemDetailWire` is the whole page's state |
| `PATCH /api/work-items/:id` | `lib/api.ts:920` | Editor save | **S** | Optimistic concurrency: `expectedVersion` + `idempotencyKey`; the client *throws* on a response without a positive version (`lib/work-item-edit-wire.ts:52`) |
| `PUT/POST /api/work-items/:id/status` | `lib/api.ts:936` | Drag / picker move | **S** | Legality **pre-checked client-side** against a mirrored edge table; `cascade:true` semantics |
| `POST /api/work-items`, `/assign`, `/archive` | `lib/api.ts:953-962` | Create / assign / archive | **S** | Assign is roster-validated server-side |
| `GET /api/work-items/:id/tree`, `GET /api/work-items/trees?ids=` | `lib/api.ts:967,987` | Board roll-ups | **S** | `WorkItemTreeWire` recursion drives expansion |
| `GET /api/departments` | `lib/api.ts:972` | Board switcher | C | |
| `GET /api/work-items/:id/sessions` | `lib/api.ts:993` | Linked attempts | C | |
| `POST /api/work-items/:id/dispatch` | `lib/api.ts:996` | Dispatch button | **S** | A `{reused: boolean}` idempotency receipt drives the copy |
| `POST /api/work-items/:id/approval` and `/approval/escalate` | `lib/api.ts:999,1008` | Approval gate | **S** | Human-only server-side; `choice` for pick-gates |
| `GET/POST/PATCH/DELETE …/comments[/:cid]` | `lib/api.ts:1014-1026` | Comment thread | **S** | Tombstone contract: a deleted row survives with an empty body plus `deletedAt`; the UI renders "[deleted]" |
| `GET/POST/DELETE …/attachments[/:aid]` | `lib/api.ts:1044-1052` | Attachments | **S** | `workItemAttachmentUrl()` returns a raw cookie-authed URL used directly as an image `src` |
| `POST/DELETE …/relations` | `lib/api.ts:1047,1050` | Relations | C | |
| `PUT …/labels`, `GET /api/labels` | `lib/api.ts:1062,1030` | Labels | C | |
| `PUT …/kept` | `lib/api.ts:1065` | Home board | C | |
| `POST /api/todo-captures`, `GET /api/todo-captures/:id` | `lib/api-todo-capture.ts:50,52` | Quick capture | **S** | A 7-stage state machine; the GET derives the stage and the event is only a nudge |

#### Workflows
| Endpoint | Called from | What the UI does | C/S | Note |
|---|---|---|---|---|
| `GET /api/workflows` | `lib/api.ts:709` | List | **S** | Cursor paging |
| `GET /api/workflows/:id` | `lib/api.ts:711` | Canvas definition | **S** | `WorkflowDefinitionWire` **imported from gateway source** |
| `PUT /api/workflows/:id`, `POST /api/workflows` | `lib/api.ts:722,724` | Editor save | **S** | `expectedRevision` → 409 |
| `GET /api/workflows/:id/runs` | `lib/api.ts:713` | Run list | C | |
| `GET …/runs/:runId` and `?view=full` | `lib/api.ts:718,722` | Run canvas | **S** | Two distinct projections: lean for polling, full carries the definition snapshot at run revision |
| `POST …/runs` | `lib/api.ts:744` | Manual run | **S** | Returns an *unprojected* shape (attempt inputs, no spend) - the client knows the difference |
| `POST …/nodes/:nodeId/approval` | `lib/api.ts:747` | Approve a node | **S** | |
| `POST …/{enable,disable,retire,unretire,duplicate}` | `lib/api-workflow-lifecycle.ts:19-27` | Lifecycle menu | **S** | All revision-guarded |
| `POST …/nodes/:nodeId/retry`, `…/cancel`, `…/rerun`, `…/attempts/:n/transcript` | `routes/workflow/**` | Run inspector | **S** | |

#### Org / cron / skills / settings
| Endpoint | Called from | What the UI does | C/S | Note |
|---|---|---|---|---|
| `GET /api/org` | `lib/api.ts:820` | Org chart | **S** | `{departments, employees, hierarchy}`; `hierarchy` is a gateway-computed tree the D3 layout consumes |
| `GET/PATCH /api/org/employees/:name` | `lib/api.ts:821,826` | Employee panel | **S** | `name` immutable; PATCH returns a re-scanned-from-disk record |
| `GET /api/cron`, `/:id/runs`, `PUT/DELETE /:id`, `POST /:id/trigger` | `lib/api.ts:814-819` | Cron pages | **S** | `CronJobWire` cast from an untyped record; the `lastRun` outcome shape drives the status pill |
| `GET/PUT /api/skills[/:name]` | `lib/api.ts:829-833` | Skills editor | **S** | The gateway returns the **raw file**; the client parses YAML frontmatter itself (`lib/skills.ts`) |
| `GET/PUT /api/config` | `lib/api-config.ts:38,46` | Settings YAML editor | **S** | An `X-Jinn-Config-Revision` request+response header is an optimistic-concurrency token |
| `GET /api/engines`, `/refresh`, `/api/engine-limits`, `/refresh` | `lib/api.ts:335,338,750-756` | Model picker | **S** | `EnginesResponse` drives the engine → model → effort cascade |
| `GET /api/features` | `lib/api.ts:706` | Feature gates (`notesEnabled`, `staleChat`) | **S** | Notes routes 404 when off |
| `GET /api/status` | `lib/api.ts:707` | Settings health | C | Unauthenticated |
| `GET/POST /api/onboarding` | `lib/api.ts:840,842` | First-run flow | **S** | An 11-field response gates route rendering |
| `GET /api/logs?n=` | `lib/api.ts:838` | Log viewer | C | |
| `POST /api/connectors/reload`, `GET /api/connectors/:id/qr` | `lib/api.ts:836`, `routes/settings/page.tsx:173` | Connector admin | C | |
| `GET/POST/PUT /api/notes`, `/api/notes/read` | `lib/api.ts:697-704` | Notes | C | |
| `GET /api/knowledge/read` | `lib/file-read-request.ts:36` | File viewer | C | |
| `GET/POST/POST /api/instances[/:id/start]` | `lib/api.ts:690-692` | Workspace switcher | **S** | Create returns a pairing-fragment URL; the client consumes and strips it (`lib/auth.ts:53`) |

#### Files, search, plugins, experiments, talk, STT/TTS
| Endpoint | Called from | What the UI does | C/S | Note |
|---|---|---|---|---|
| `POST /api/files` | `lib/api.ts:1071` | Chat upload | **S** | `sessionId` routes it into the date-bucketed uploads dir; the returned URL is what reconciliation matches on |
| `GET /api/files/:id` (+ `?poster=1`, `?quality=low`, `?download=1`) | `components/chat/message-media.tsx`, `components/ui/video-player.tsx` | Inline media | **S** | Query-param variant vocabulary plus byte-range/ETag behaviour assumed |
| `GET /api/files/read` | `lib/file-read-request.ts:43` | File route | C | |
| `GET /api/search/global` | `lib/search-api.ts:89` | Command palette | **S** | The client re-declares the gateway's `search/types.ts` verbatim; **result order is the contract** - the kind list is presentation order, and results arrive grouped by kind in exactly that order; highlight snippets are parsed into nodes |
| `GET /api/search/messages` | `components/talk/tools/chat-message-search.ts:54` | Talk tool | C | |
| `GET /api/plugins`, `POST /api/plugins/rescan`, `POST /api/plugins/:id/enabled`, `/reveal` | `plugins/disk-plugins.ts:17`, `routes/settings/plugins/inventory.ts` | Plugin manager | **S** | |
| `GET /api/plugins/:id/client`, `/assets/*` | loaded as ESM by `plugins/disk-plugins.ts:95` | **Runtime code loading** | **S** | The gateway JSX-transforms and serves executable plugin code the SPA imports |
| `ANY /api/plugins/:id/<tail>` | `plugins/plugin-context.ts:95` | Plugin backend calls | **S** | A `..`-segment refusal (encoded forms too) enforces the mount namespace |
| `GET/POST/PATCH /api/experiments[/:id][/readings][/conclude]` | `lib/api-experiments.ts:21-29` | Experiments | C | Types come from `@jinn/gateway-events` |
| `POST /api/talk/sessions` plus `GET/DELETE /:id` plus `/turn /transcript /context /interruptions /park /resume /token /heartbeat /actions /control /handoff` | `components/talk/transport/session-client.ts:73-270` | Voice session | **S** | The deepest lifecycle in the app: a credential generation, a `live | parked` state, a server-supplied **control manifest** the client executes verbs against, and a typed `{reason:"unconfigured"}` refusal |
| `POST /api/talk/proactive/:id/ack` | `components/talk/transport/session-client.ts:163` | Cue ack | **S** | |
| `GET /api/talk/config` | `lib/talk-capability.ts:22` | Capability probe | C | |
| `GET/POST /api/tts` | `components/chat/tts-engine.ts:186,195` | Speech playback | **S** | GET is a capability probe; a 503 with `{available:false}` falls back to Web Speech |
| `GET /api/stt/status`, `POST /download`, `POST /transcribe`, `PUT /config` | `lib/api-stt.ts:25-51` | Dictation | C | |

**Server routes the web client never calls** (an MCP/CLI surface the new API seam does not have
to satisfy for the view layer): `/api/cost/report`, `/api/delegations`, `/api/system/restart`,
`/api/callback-deliveries/*`, `/api/heartbeats*`, `/api/sessions/:id/context`,
`/api/work-items/:id/dispatch-config`, `/api/work-items/:id/approval/request`,
`/api/workflows/events/:name`, `/api/workflows/attempts/{submit,extend}`, `/api/internal/hook`.

### 3.2 Realtime

**No SSE, no polling as the primary channel.** One WebSocket per app, plus two special-purpose
sockets. Grepping for `EventSource` / `text/event-stream` returns nothing.

| Socket | Opened at | Purpose |
|---|---|---|
| `/ws` | `lib/ws.ts:99`, one per app via `GatewayProvider` (`hooks/use-gateway.tsx`) | The event bus |
| `/ws/pty/:sessionId` | `components/cli-terminal.tsx:241` | xterm CLI view |
| `/api/plugins/:id/events` | `plugins/plugin-context.ts:118` | Plugin event ring, `?since=<cursor>` replay |

**Event contract.** `packages/gateway-events/src/index.ts` is a **shared workspace package**
(`@jinn/gateway-events`, a real dependency of `packages/web`) declaring a 33-name event map, a
`GATEWAY_EVENTS` constant, per-event runtime payload guards, and `decodeGatewayEvent()`. A
frame failing its guard is **silently dropped** (`lib/ws.ts:127`). The full list:

`session:{started,created,updated,deleted,stopped,external-turn,interrupted,completed,delta,notification,attachment,background}`,
`queue:updated`, `company:changed`, `pins:changed`, `notes:changed`, `experiments:changed`,
`todo-capture:stage`, `org:changed`, `config:reloaded`, `skills:changed`, `plugins:changed`,
`plugin:notice`, `cron:{reloaded,run-finished}`, `engines:updated`,
`stt:download:{progress,complete,error}`, `talk:audio`,
`talk:tts:download:{progress,complete,error}`, `talk:proactive-cue`.

The two carrying real structure:
- **`session:delta`** - `{sessionId, type: 'text'|'text_snapshot'|'tool_use'|'tool_result'|'status'|'error'|'context'|'block', content, toolName?, toolId?, activityReceiptId?, block?}`. Eight sub-types, each with different transcript semantics.
- **`company:changed`** - a three-arm discriminated union (`todo` with `{id, version, value}` / `workflow-definition` with `{id, revision}` / `workflow-run` with `{workflowId, runId}`).

**Liveness, reconnect, backoff.**
- App-level ping every 25s; the gateway echoes a pong (`lib/ws.ts:9`). Pongs are consumed, never dispatched.
- **Silence watchdog** at 60s: any inbound frame re-arms it; on expiry the socket is force-closed to trigger reconnect (`lib/ws.ts:62`).
- **Backoff**: exponential with *equal jitter* - the delay lands in `[window/2, window]` where `window = min(30s, 1000 · 2^attempt)` (`lib/ws-backoff.ts`). The floor is deliberate: full jitter would let a delay collapse to ~0.
- **Resume triggers**: `visibilitychange`, `online`, and `pageshow` (iOS bfcache) all reconnect if not open (`hooks/use-gateway.tsx:97-104`). Handlers are guarded by socket identity so a superseded socket's late events are no-ops.

**What happens on reconnect - three reconciliation layers.**
1. **Global** (`hooks/use-query-invalidation.ts:265-273`): every open bumps a connection sequence, which unconditionally invalidates `todos`, `workflows.all`, `sessions.all`.
2. **Per-pane backfill** (`hooks/use-live-session.ts:1239-1251`): a 300ms-debounced `loadSession()` fires *only* when the session status is `running`/`interrupted` **or** a turn is locally in flight.
3. **Dropped-completion watchdog** (`use-live-session.ts:1253-1288`): if still loading, no delta for the watchdog window, and the server says status is not running, force-settle.

Steady-state invalidation is a **trailing 1s debounce with a hard 2s ceiling**, and todo keys
are *deferred* while a todo mutation is in flight so a refetch cannot clobber an optimistic
view (`use-query-invalidation.ts:70-80`).

Genuine polling exists in only four places: cron list and detail at 60s
(`routes/cron/page.tsx:137`, `detail.tsx:72`), workspaces at 30s (`hooks/use-workspaces.ts:10`),
and a 10s "is the turn still running" probe armed only while loading
(`use-live-session.ts:1293-1315`).

**Correctness that depends on ORDERING or on a status field arriving.** This is the sharp edge,
in five distinct places:

1. **The known one - `status:'running'` must arrive during a turn.** Loading-gated UI is seeded from `session.status === 'running'` in three places: the prefetch snapshot (`use-live-session.ts:479-485`), mount from cache (`:1156-1161`), and read-only consumers. `chat-pane.tsx:152` computes `turnRunning: loading || turnPending` and `chat-pane-title-bar.tsx:56` derives the pane status from it. If `running` never lands, `loading` starts false and the un-hidden UI shows a finished-looking pane mid-turn. The client compensates three ways - `session:started` sets `status:'running'` locally (`:710`), a read-only consumer treats any delta as proof of running (`:717`), and the 10s poll - but each is a patch over a missing status, not a replacement. The gateway's own note confirms the invariant: the POST handler sets `status:"running"` and lastActivity **synchronously at enqueue** (`packages/jinn/src/gateway/status-reconciler.ts:13-15`), and a separate 15s reconciler exists purely to unstick sessions where the completion event was lost.
2. **Block versioning.** `shouldIgnoreBlockUpdate` (`lib/blocks.ts:281`) drops any envelope with a lower `version`; at equal versions, activity blocks additionally compare `activityOrder`, and an incoming block *without* `activityOrder` is dropped. Out-of-order or unversioned block frames are silently discarded.
3. **Todo version fencing.** `mergeTodoIntoCaches` applies `company:changed` patches version-aware so an older event can never overwrite a newer cached revision - but the patch alone cannot insert a *created* Todo, so every todo event **also** schedules the debounced reconciliation pass (`use-query-invalidation.ts:26-35`).
4. **`tool_use` → `tool_result` pairing.** A `tool_result` mutates the message the matching `tool_use` created, correlated by `toolId`, falling back to a **reverse scan for the last unmatched `toolName`** (`use-live-session.ts:772-786`). Reordered or duplicated tool frames mis-pair.
5. **Streaming-text flush order.** Accumulated streaming text must be flushed into a settled message *before* a `tool_use` or `block` frame appends (`:734-748`, `:836-852`), otherwise the tail interleaves wrongly. And `text_snapshot` replaces unconditionally - a deliberate removal of a length gate, so a *shorter* snapshot (a redaction) must win.

Related: `reconcileMessages` (`lib/conversations.ts:106`) assumes **both arrays are
timestamp-sorted** and consumes local rows per match so two identical messages cannot both
adopt the same optimistic id.

`session:background` is the one event handled with a pure surgical cache patch and no
invalidation - except for delegated children, whose runtime state rolls up into parent
summaries (`use-query-invalidation.ts:215-240`).

### 3.3 Auth and session assumptions

**Model:** HttpOnly cookie pairing. No bearer token anywhere in the browser client - grepping
for `Bearer` finds only two redaction regexes and one *outbound* header to the third-party
realtime provider (`components/talk/transport/webrtc-connection.ts:113`), using an ephemeral
credential the gateway mints.

**Cookies** (`packages/jinn/src/gateway/auth.ts:317-321`): `jinn_auth` plus `jinn_device`, both
`Path=/; HttpOnly; SameSite=Lax; Max-Age=31536000`. Crucially they are **namespaced per
instance home** - RFC 6265 scopes cookies by host but not port, so two gateways on one host
would log each other out; the default home keeps the bare names, every other home gets a
`_<basename>` suffix (`auth.ts:292-310`).

**Whether auth is required at all is host-derived**, not configured independently:
`shouldRequireGatewayAuth` returns true if `gateway.authRequired` is set **or** `gateway.host`
is a non-loopback bind (`auth.ts:244,267`). A loopback gateway is open by default.

**Unauthenticated behaviour.** `AuthGate` (`routes/auth-provider.tsx:138`) renders one of three
things and nothing else: a "Checking gateway access…" interstitial, the full app, or the
pairing screen. The four-step boot in `refresh()`:
1. `GET /api/auth/state`.
2. Consume a pairing fragment if present - always consumed even when not needed, so a credential never lingers in the address bar - and redeem it.
3. If still unauthenticated and `canBootstrapLocal`, consume a bootstrap fragment and POST it with the grant header.
4. Re-read state; paired means render.

**Transparent 401 retry.** `authFetch` (`lib/auth.ts:157`) intercepts any 401, re-reads auth
state, attempts local bootstrap, and replays the original request - guarded by
`assertGatewayProfile` so a native profile switch mid-flight aborts rather than authenticating
one profile's request against another.

**Server-side gates a new host must reproduce:**
- **CORS is restrictive and 403s**, not a silent header omission: only a loopback-hostname origin, or an origin whose hostname equals the request `Host`, is allowed (`request-handler.ts:29-42,91-95`). Allowed request headers are exactly `Content-Type, Authorization, X-Jinn-Bootstrap-Grant, X-Jinn-Config-Revision`; `X-Jinn-Config-Revision` is the only exposed response header - the config editor's concurrency token stops working without it.
- **Loopback-only endpoints**: bootstrap, pairing-challenges, pairing-codes, internal hook - checked on *both* socket address and `Host` header.
- **`isSameOriginBrowserRequest`** (`packages/jinn/src/gateway/api.ts:881`) grants operator authority to the local dashboard for writes and the PTY upgrade when auth is not required. It demands loopback on both socket ends, no forwarded headers, `Host` matching the actual listener address **and port**, and `Sec-Fetch-Site: same-origin` plus `Sec-Fetch-Mode: cors|websocket` plus `Sec-Fetch-Dest: empty`.
- **Operator vs employee control plane**: roughly 30 routes (config write, session delete/duplicate/archive, all queue mutations, all cron writes, org employee update, skill write, plugin enable/rescan, STT download, workspace create) are operator-only and 403 for an identified employee session (`api.ts:950-984`).
- **Socket upgrade auth**: `/ws` and `/ws/pty/*` are both matched by the auth gate, so an unauthenticated upgrade gets a raw 401 on the socket. `/api/plugins/:id/events` deliberately carries **no token in its URL** because the single upgrade gate already authenticated it (`plugins/plugin-context.ts:99-104`).
- `POST /api/auth/pairing-codes` **refuses bearer callers** with 403 - bearer tokens cannot mint browser pairing material.

**Native path.** A second transport implementation (`lib/native-gateway-transport.ts`,
`native-gateway-profiles.ts`) tunnels HTTP and WebSocket over a native bridge with base64
bodies, holds multiple paired gateway *profiles*, and wraps every socket in a `GuardedSocket`
that goes silent the moment its gateway stops being active (`lib/native-gateway-socket.ts`). A
new host must be reachable by a bare origin - the native transport *rejects* an origin with any
path, query, fragment, or credentials.

### 3.4 Data-shape coupling - the structural list

Ordered by depth. This is the expensive list.

#### Tier 1 - the wire type IS the feature's state model (whole subtree)

| Component | Shape mirrored | Depth |
|---|---|---|
| **Todos task page** - `routes/todos/task-page/*` (20 files) | `WorkItemDetailWire`, `WorkItemCommentWire`, `WorkItemAttachmentWire`, `WorkItemRunWire`, `WorkItemApprovalWire`, `WorkItemTreeNodeWire`, `WorkItemLabelWire` | **Whole feature.** Wire types reach the leaves: `attachment-preview.tsx` names 14 `*Wire` types, `activity.tsx` 12, `runs.tsx` 7, `relations.tsx` 7, and even `label-chip.tsx` and `subtask-row.tsx` are wire-typed. `routes/todos/` plus `routes/workflow/` together carry **125** imports from `@/lib/api`. |
| **Chat transcript** - `hooks/use-live-session.ts` (1428 lines) plus `chat-messages.tsx` (900+) plus `chat-blocks.tsx`, `handoff-card.tsx`, `company-activity-card.tsx`, `todo-activity-burst.tsx`, `dispatch-row.tsx`, `message-arrival.ts` | the `session:delta` union, `ChatBlock`/`ChatBlockEnvelope`, `MessageMediaWire` | **Whole feature.** `ChatBlock` is not adapted at any boundary - six leaf card components take it as a prop. The hook is a hand-written reducer over eight delta sub-types plus five session events. |
| **Workflow canvas plus run inspector** - `routes/workflow/{run,run-canvas,run-inspector,run-attempt-card,run-rounds,editor/*}.tsx` | 24 types imported **directly from `../jinn/src/workflows/wire.ts`** via a Vite path alias | **Whole feature**, and the worst kind: not a mirrored copy but a compile-time dependency on the gateway package's source file. |
| **Talk transport** - `components/talk/transport/*` | `OpenTalkSession`, `ResumableTalkSession`, the server-supplied **control manifest**, `TalkProactiveCuePayload` | **Whole feature.** The client executes verbs the *server* declares in the manifest, and validates the resumable-session shape field by field, throwing on mismatch (`session-client.ts:139-150`). |

#### Tier 2 - subtree, the wire type flows two or more levels

| Component | Shape | Depth |
|---|---|---|
| `components/chat/chat-sidebar.tsx` (1400+ lines), `session-signals.tsx`, `mobile-session-row.tsx`, `routes/chat/session-picker.tsx` | The **untyped** session row | Subtree, with *no type boundary at all*. Components read `session.employee`, `.status`, `.title`, `.lastActivity`, `.createdAt`, `.turnProgress`, `.queueDepth`, `.transportState`, `.backgroundActivity`, `.parentSessionId` straight off gateway JSON. Adding a field to the gateway silently changes the sidebar; removing one silently breaks it. |
| `routes/org/page.tsx` plus the org tree | `OrgData = {departments, employees, hierarchy}` with `Employee` carrying gateway-computed `parentName`, `directReports`, `depth`, `chain` | Subtree - the hierarchy is *not* derived client-side; the D3 layout consumes the gateway's tree. |
| `routes/cron/{page,detail}.tsx` | `CronJobWire`, cast from an untyped record at `page.tsx:139` | Subtree; the `lastRun` outcome shape drives the status pill and optimistic toggle patching. |
| `routes/todos/board/**`, `list/**` | `WorkItemCompactWire`, `WorkItemTreeWire` | Subtree - the board's column model *is* the status vocabulary. |

#### Tier 3 - mirrored gateway algorithms (not types - logic)

Four modules in `src/lib/` explicitly re-implement gateway code. These are the ones that will
rot silently under a new host.

| File | Mirrors | How it is held together |
|---|---|---|
| `lib/transition-edges.json` plus `lib/legal-targets.ts` | `packages/jinn/src/work-items/transitions.ts` - the Todo status state machine, edge for edge, plus the `manualExecutingFrom` / `sticky` / `closeGated` rule constants | A **cross-package parity test** (`packages/jinn/src/work-items/__tests__/board-legality-parity.test.ts`) probes the real `transition()` against the JSON and fails the build on drift. The UI pre-checks legality so a drag can be refused before it is sent. |
| `lib/attachment-ref.ts` | `packages/jinn/src/workflows/attachment-ref.ts` - the attachment-ref grammar including the id shapes | "The two must agree, and the rejections are what matter" - a lax parser lets a smuggled token reach an image `src`. |
| `lib/search-api.ts` | `packages/jinn/src/search/types.ts` | "The gateway owns the contract and no shared package exports it, so a change there is a change here." |
| `lib/blocks.ts` | The gateway's block-envelope merge/version rules plus a payload safety allowlist (forbidden keys, depth ≤ 8, char caps) | Fallback-content contract: when an envelope arrives with no text, the client synthesises a message from `blockFallbackContent()` and marks it synthetic by id prefix so a later patch can replace the text - and a remove deletes the whole message if it was synthetic. |

Plus three **build-time aliases into gateway source** (`vite.config.ts:126-134`):
`@jinn/workflow-wire` → `../jinn/src/workflows/wire.ts` (types only), `@jinn/fallback-map-wire`
and `@jinn/model-id` → `../jinn/src/shared/*` (**real runtime code the bundle carries**).

**The five worst offenders**
1. `routes/todos/task-page/**` - wire types to the leaves, 20 files.
2. `hooks/use-live-session.ts` - 1428 lines of hand-written reducer over the delta union, with three reconciliation layers.
3. `routes/workflow/**` - compiles against gateway source files by path.
4. `components/chat/chat-sidebar.tsx` plus `session-signals.tsx` - untyped gateway JSON with no adapter layer.
5. `lib/legal-targets.ts` plus `transition-edges.json` - the gateway's state machine, duplicated and build-enforced.

### 3.5 Hardcoded host and port assumptions

The client itself is **origin-agnostic by construction**, which is the good news.
`createBrowserGatewayTransport` derives everything from `window.location.origin`, forces paths
to be root-relative, and *throws* if a resolved URL leaves the profile origin
(`lib/gateway-transport.ts:44-58`). Socket URLs are derived by protocol swap. Every request
goes out with `credentials: "include"`.

| Assumption | Location | Nature |
|---|---|---|
| **Port 7777 default, loopback dev proxy** | `vite.config.ts:46,174-183` - a gateway-port env var defaulting to 7777, proxying `/api` and `/ws` to loopback | Dev-only, env-overridable. **But** the gateway knows about it: `originMatchesLoopbackViteProxy` (`packages/jinn/src/gateway/api.ts:907`) is a named carve-out in the same-origin trust rule. |
| **`/api` and `/ws` path prefixes** | Everywhere; `/api` is the gateway's `startsWith` dispatch discriminator (`request-handler.ts:107`) - anything else falls through to the SPA | Hard structural assumption: the new host must serve the app and the API on **one origin**, with `/api/*` reserved. |
| A loopback placeholder port in the native pairing screen | `components/auth/native-pairing-screen.tsx:49,76,267` | Cosmetic placeholder text |
| Loopback/port placeholders in settings forms | `routes/settings/page.tsx:534,541` | Cosmetic form placeholders |
| Loopback hostname allowlist | `packages/jinn/src/gateway/auth.ts:231-242` | Server-side; determines whether auth is required at all |
| Native profiles require a **bare origin** | `lib/native-gateway-transport.ts:11-18` | Rejects any path, query, fragment or credentials |
| PWA precache manifest names build-output patterns | `vite.config.ts` workbox `globPatterns` | Build-output naming, not host |

No hardcoded absolute URLs, no env-var API base, and no absolute `fetch()` in application code.

### 3.6 The verdict

1. **Must match exactly to reuse the view layer verbatim:** the `@jinn/gateway-events` event map - all 33 names, the eight `session:delta` sub-types, and the `company:changed` three-arm union. Frames failing the payload guards are dropped silently, so a near-miss shows as a dead UI, not an error.
2. **Also exact:** `status: 'running'` must be set *synchronously at enqueue* and cleared on completion. Three separate client mechanisms exist only to survive its absence; none is a substitute.
3. **Also exact:** the version/revision fences - Todo `expectedVersion` plus `idempotencyKey`, workflow `expectedRevision`, `X-Jinn-Config-Revision`, and block `version`/`activityOrder`. The Todo edit client *throws* on a response without a positive version.
4. **Also exact:** the Todo status edge table. It is duplicated in the client and build-enforced by a parity test; a new host with different edges makes the board pre-check lie.
5. **Also exact:** `/api/*` and `/ws` on the **same origin** as the app, with cookie-based auth and the four allowed request headers. A split origin needs a real CORS/credentials design, not a config change.
6. **Adaptable cheaply:** roughly 40% of endpoints - pins, queue, notes, knowledge, labels, relations, skills list, STT, logs, connectors, experiments, cron reads. URL-only.
7. **Adaptable with an adapter layer:** the org tree, cron, and search surfaces. One mapping function each; the wire shape stops one or two levels in.
8. **Must be rewritten or matched wholesale:** the chat transcript pipeline (`use-live-session.ts` plus `blocks.ts`, ~1800 lines of ordering-sensitive reducer), the Todos task page (wire types to the leaves), and the Talk transport (server-declared control manifest).
9. **The single hardest coupling is not an endpoint:** `vite.config.ts` path-aliases the web build into `../jinn/src/` for workflow wire types, the fallback map and the model id. The workflow feature does not consume an API - it compiles against the daemon's source tree.
10. **Natural seam:** the four `*-wire.ts` files in `src/lib/` are already a deliberate, documented wire layer (approval, comment, edit, runs) - self-contained leaf modules, re-exported through `api.ts` so no caller moved. Extending that pattern to sessions (today untyped, with no boundary at all) is the highest-leverage decoupling move available.
## 4. Malleability read

Target side is `jinn-harness` at `e6a7935`, kernel pin `3a8e5c0` (`KERNEL-PIN.md`). Harness
paths are unprefixed (`plugins/…`, `docs/notes/…`); web paths always begin `packages/web/`.

### 4.0 The one mechanism fact everything below rests on

The kernel's event bus already has the shape the direction needs. `kernel-pin/wit/plugin.wit`
(`interface types`) declares `enum dispatch-mode { emit, parallel, serial, bail, waterfall }`,
and `interface events`'s `emit: func(topic, mode, target, payload) -> result<list<list<u8>>, kernel-error>`
returns listener outputs. A listener answers with bytes: `interface lifecycle`'s
`handle-event: func(token, topic, payload) -> result<list<u8>, guest-fault>`. So a waterfall
listener authored in JS inside a WASM plugin needs **no kernel change and no constitutional
amendment** - the return channel exists at the pin.

Three facts qualify that, and every moment in 4.3 inherits them:

1. **The harness has never used `waterfall`.** Grepping `plugins/`, `tools/` and `tests/` for `waterfall` returns nothing. Every seam emits `DispatchMode::Emit` (`plugins/sessions/store-core/store.rs:125`, `plugins/todos/store-core/store.rs`, `plugins/workflows/store-core/store.rs:169`, `plugins/engines/jinn-engine-claude/src/lib.rs:124`); the only non-`Emit` use in the tree is `DispatchMode::Serial` on the settings hot path (`plugins/settings/jinn-settings-profile/src/lib.rs:278`). The mode is contract, not practice.
2. **A reply-expecting dispatch is refused whole if any selected listener owes a transition** (`kernel-error`'s `restarting` / `gone` / `suspended` / `stalled`, each carrying a `refused-target`; pin `3a8e5c0` = M2-K9). A user extension that is mid-reload silently *blocks* the moment it is attached to rather than being skipped. Every interception point therefore needs a stated fail-open or fail-closed policy.
3. **`FINDINGS.md` #4/#32 is unretired.** "The kernel awaits every listener delivery end-to-end in every mode, so fire-and-forget discards the ANSWER, never the WAIT" (`docs/notes/2026-08-30-sessions-seam-definition.md`). A listener that calls back into the emitting instance while handling deadlocks to the 5s guest deadline. This is the single largest design constraint on gateway-side interception: **an extension may transform its payload, but it must not re-enter the seam that emitted it.**

The existing web-side plugin event surface is not a waterfall and never was.
`packages/web/src/plugins/sdk/host-events.ts` declares `type HostEventHandler = (frame: HostEvent) => void`,
a `void` return, fanned out by `dispatchHostEvent` from the app's one gateway socket.
Read-only notification, inbound only. Nothing in `packages/web/src/plugins/**` can change a
value on its way past.

### 4.1 Seam inventory (target side)

**`api` - `plugins/api/`.** The operator surface. `jinn-api` is the pure definition: operation names, additive request/answer schemas, the typed error/answer envelope, the entry-patch law (RFC 7396 merge on **one entry's `config` subtree only**), and the route table (`plugins/api/jinn-api/src/lib.rs` ROUTES plus `engines.rs`, `sessions.rs`, `todos.rs`, `workflows.rs`, `plugins.rs`). `jinn-api-http` owns transport alone: one `jinn:net` loopback listener served from readiness wakes, minimal HTTP/1.1 + JSON via `jinn-api-http-wire`. `jinn-status` and `jinn-profile-edit` provide `jinn:api-status` / `jinn:api-profile`. Wire shape: `GET|POST|PATCH /v1/{status,health,ledger/tail,profile,profile/entries/{id},settings[/{ns}],engines…,sessions…,todos…,workflows…,plugins…}`, JSON bodies capped at 256 KiB, heads at 16 KiB. Defining doc: `docs/notes/2026-08-29-operator-api-seam.md`. **Hard limits, from `README.md`:** no authentication or authorization at all (loopback plus the granted port *is* the boundary); no keep-alive (`jinn-api-http-wire` header: "every response closes"); no chunked encoding; no server push, no SSE, no WebSocket; **no static-file serving of any kind**; and `jinn:net` v0.1 has no outbound `request`, so connectors are structurally impossible in this repo today.

**`settings` - `plugins/settings/`.** Per-plugin settings as a capability. `jinn-settings` owns namespace declarations (schema, defaults, hot keys), a closed schema language and validator, typed secret references (`{"$secret": "<key>"}` - a name, never a value), layered resolution `defaults < owner entry < overlay`, the patch plan (which layer a patch lands in, hence whether the owner restarts), and the `changed`/`refused` payloads. It is also the distribution's one home for the additivity/closed-surface wire law. `jinn-settings-profile` writes through `jinn:profile.patch-entry`; `jinn-settings-store` holds the hot overlay layer. Topics: `jinn:settings/changed`, `jinn:settings/refused`. Wire: `GET /v1/settings`, `GET|PATCH /v1/settings/{ns}`. Docs: `docs/notes/2026-08-29-settings-seam.md`, `docs/notes/2026-08-29-closed-surfaces-refuse.md`. Limits: a mixed hot+cold patch across two layers cannot be applied atomically (#28); secret references have no rotation or revocation lifecycle.

**`engines` - `plugins/engines/`.** Coding agents. `jinn-engine` defines the contract `jinn:engine.<id>` - the id is in the contract name, which is what makes switch/coexist/extend all profile edits (`plugins/engines/README.md`). `RunRequest` (`plugins/engines/jinn-engine/src/lib.rs:212`) carries `engine`, `model`, `effort`, `prompt` (delivered on stdin, never argv), cwd, tool policy, budget, and secrets as keystore references. Operations `describe`/`run`/`run-get`/`cancel`; events on `jinn:engine/event`. Providers: `jinn-engine-claude`, `jinn-engine-codex` (real CLIs through `jinn:process` under an exec allowlist plus an env allowlist), `jinn-engine-echo`. Docs: `docs/notes/2026-08-29-engines-seam.md`, `…-engines-additivity-and-lifecycle-proofs.md`. Limits: `run-get` cannot tell a reaped run from an id that never existed; the echo provider's token counts and zero cost are stand-ins, so every CI-runnable usage proof runs on fabricated numbers.

**`sessions` - `plugins/sessions/`.** Durable conversations, and the first seam that composes another. `jinn:session.<store-id>`; `SessionSpec` (`plugins/sessions/jinn-session/src/spec.rs:41`) = `engine: EngineBinding`, `cwd`, `tools: ToolPolicy` (default-deny), `attribution`, `metadata`, `extra`. `SendRequest` (`:86`) = `{ session-id, message, …extra }`; `send` answers `TurnAccepted { session-id, turn-id }` at once and progress arrives on `jinn:session/event`. Ops: `create`/`send`/`get`/`messages`/`list`/`cancel`/`close`. Providers `jinn-session-fs` (one append-only JSONL journal per session) and `jinn-session-memory`. Wire: `/v1/sessions/{store}[/{id}[/turns|/messages|/events]]`. Docs: `docs/notes/2026-08-30-sessions-seam-definition.md`, `…-sessions-seam-stores.md`. Limits: one unreplayable journal takes the whole durable store down; **the event feed is a cursor read, not a push**; a store polls its engine; the ring is bounded and reports its drops.

**`todos` - `plugins/todos/`.** The work ledger, three layers deep (`jinn:todo.<store>` → `jinn:session.<store>` → `jinn:engine.<id>`), the layering enforced by *authority* - a Todo store's entry is granted no `jinn:engine.<id>` at all (`tools/todo-kit`). The status law is an explicit table (`plugins/todos/jinn-todo/src/status.rs`): statuses `backlog | executing | in-review | blocked | done | cancelled`; a producer does not close their own work (`executing → done` is not a move); `done`/`cancelled` are terminal with **no exits**; `x → x` is in no row. A refused move is typed with `from`/`to` as data and is *recorded before the caller is told* - `Todos::plan_update` answers `Moved::Refused` carrying its own record. Ops: `create`/`update`/`comment`/`get`/`list`/`tree`/`dispatch`/`events`; topic `jinn:todo/event`. Docs: `docs/notes/2026-08-30-todos-the-fold-is-not-enough.md`, `…-todos-the-order-is-the-guarantee.md`. Limits: no edit and no delete, on a Todo or a comment.

**`workflows` - `plugins/workflows/`.** The reusable HOW, four layers deep. `jinn:workflow.<store-id>`: spec (nodes, edges, typed input schema), node-state transition table, run-status space, ops `describe`/`define`/`get`/`list`/`start`/`get-run`/`list-runs`/`node-state`/`cancel`/`events`, topic `jinn:workflow/event`. A run is **pinned to one definition revision for its whole life** and carries that revision's whole spec in its own `run-started` line (`plugins/workflows/jinn-workflow/src/revision.rs`). Recovery is an *order*, not a fold: a durable store replays, appends the `running → interrupted` moves and the run's ending, and only then provides its contract. Docs: `docs/notes/2026-08-30-workflows-the-pin-and-the-fourth-layer.md`, `…-workflows-a-run-is-a-positive-reading.md`. Limits: no retry, no delete; a run is not resumed across a restart (a decision); the graph walk is proven on two shapes through the daemon, wide fan-out and deep chains unit-proven only; `spec-digest` is a 64-bit FNV-1a change detector, not a hash.

**`cron` - `plugins/cron/`.** Phase 1's seam, never revisited as a product surface. `jinn-cron` defines the settings namespace, `JobSpec`/`JobConfig`, `FirePayload`, the run-record shape and the firing law; ops are only `jobs` and `history` (`plugins/cron/jinn-cron/src/lib.rs:27,29`). `cron-scheduler` holds one `jinn:clock` periodic alarm at `tick-ms`, plans at activate and on every wake, emits typed fire events on each job's own topic, and consumes its job table through `jinn:settings`. `health-snapshot` is the real consumer. Doc: `docs/notes/2026-08-28-cron-seam-design.md`. **Wire gap worth naming: there is no `/v1/cron` route.** Cron is reachable over HTTP only as `PATCH /v1/settings/cron` and as a probe row inside `GET /v1/status`. It is also the only seam with soak evidence (`SOAK.md`); no 2.x seam has any.

**`plugins` - `plugins/plugins/`.** The plugin tree as a capability. `jinn:plugins.<catalog-id>`: the lifecycle *reading* law and its transition table, the grant reading and its source, the ledger attribution rule, the read window every answer carries; ops `list`/`describe`/`history`/`describe-catalog`. `jinn-plugins-profile` derives its entry set from the document of record; `jinn-plugins-static` from its own config. Wire: `GET /v1/plugins[/{catalog}[/{id}[/history]]]`. Docs: `docs/notes/2026-08-31-the-catalog-is-the-swappable-part.md`, `…-a-reason-is-not-a-neighbour.md`, `…-absence-is-three-things.md`. **Limits that bear directly on a UI:** the surface is **read-only** (no enable, disable, restart, remove - deliberately: reshaping is `PATCH /v1/profile/entries/{id}`); **there are no typed events at all**, a recorded decision (#40) because the kernel is not a publisher on the bus and there is no lifecycle listen topic; three of eleven readings (`mounted`, `activating`, `interrupted`) are **unreachable at this pin** (#41 - 189 consecutive reads all said `active` across a real restart); an entry the document could not resolve appears in **no catalog at all**; and an API-driven provider swap works only because this seam designed its binding into `config` - `patch-entry` writes `config` and nothing else (#37).

### 4.2 Surface → seam mapping

| Web surface | Harness seam | Fit | What is missing |
|---|---|---|---|
| **Chat** (`routes/chat/page.tsx`, `components/chat/chat-pane.tsx`) | `sessions` (+ `engines`) | **partial** | The nouns line up (`create`/`send`/`messages`/`cancel`/`close` ≈ `createSession`/`sendMessage`/`getSession`/`stopSession`). What is absent is the *live* half: the web app runs one WebSocket (`hooks/use-gateway.tsx:47`, `lib/ws.ts`) and `useLiveSession` consumes `session:delta` text/tool/context/block frames; the harness feed is a **bounded-ring cursor read**, not a push, and the HTTP wire has no keep-alive, no chunking, no SSE. Also missing: message media/attachments (no upload route, no blob surface on the API), `interrupt`, `speech` provenance, interactive/PTY mode, employees, and the whole block/BlockKit envelope (`lib/blocks.ts`). |
| **Todos** (`routes/todos/**`) | `todos` | **partial** | Vocabulary divergence is real and load-bearing: the web app's statuses are `backlog, assigned, executing, in_review, blocked, done, cancelled, escalated` with `done → backlog` reopening allowed (`lib/transition-edges.json`, `lib/legal-targets.ts`), while `jinn-todo`'s closed space is six statuses with **terminal-is-terminal** and no `assigned`/`escalated`. Also missing: approvals (`lib/work-item-approval-wire.ts`), labels, attachments, edit/delete, boards, assignment, search, and optimistic-concurrency versions. |
| **Workflows** (`routes/workflow/**`) | `workflows` | **partial** | Definition/run/node-state/events all exist. Missing on the target side: **triggers** (nothing binds an event or a poll to a workflow), enable/disable/retire/duplicate (`lib/api-workflow-lifecycle.ts` - four writes with no counterpart), node retry, run approvals (`routes/workflow/approval-decision.tsx`), and the revision-guarded `expectedRevision` 409 contract. |
| **Cron** (`routes/cron/**`, `hooks/use-cron.ts`) | `cron` | **partial** | The scheduler and run history exist as a capability, but **there is no HTTP route** - the UI's list/detail/run-row would have to read the `cron` settings namespace and the status probe row. No create/edit/delete/enable/run-now over the wire; no per-job run detail endpoint. |
| **Settings** (`routes/settings/**`, `routes/settings-provider.tsx`) | `settings` | **direct** | The closest fit in the whole table: declared namespaces, schemas, layered resolution, hot vs restart patch plan, typed refusals. What the *app's* settings page carries beyond it - appearance/accent/text-scale, emoji rows, engine config forms - are app-level preferences with no namespace declared yet, which is a declaration to write rather than a seam to build. |
| **Org / employees** (`routes/org/page.tsx`, `hooks/use-employees.ts`) | n/a | **no seam yet** | `README.md` names it: the org and its employees, delegation and approvals have **no plugin in this repo**. No persona store, no hierarchy resolution, no `reportsTo`, no delegation lane. `SessionSpec.attribution` is the only adjacent field, and it is metadata, not an org. |
| **Notes** (`routes/notes/**`) | n/a | **no seam yet** | No notes capability. A `jinn:fs` grant plus a store provider is the obvious shape and none of it exists. |
| **Skills** (`routes/skills/**`, `hooks/use-skills.ts`) | n/a | **no seam yet** | No skill discovery, no SKILL.md reader, no symlink sync. Adjacent only through `SessionSpec.tools: ToolPolicy` (default-deny), which names tools, not skills. |
| **Files** (`routes/file/page.tsx`, `components/chat/file-view.tsx`) | n/a | **no seam yet** | `jinn:fs` exists as a *kernel capability granted per entry under the data root*; there is no operator-facing file read/write/upload contract, and no route serves bytes. |
| **Logs / activity** (`routes/logs/page.tsx`) | `api` (`jinn-status`) | **partial** | `GET /v1/ledger/tail` gives the kernel's own ledger. That is a different artifact from what the page renders (session and company activity). No filtering, no cursor beyond the tail, and the ledger's own 500-line cap sits above any window. |
| **Limits / usage** (`routes/limits/page.tsx`) | `engines` | **partial** | `RunRequest` carries a budget and `Runs` does budget accounting, so the concept exists per-run. There is no aggregate usage surface, no quota/window model, and the echo provider's costs are stand-ins - so any limits UI built on today's numbers would be reading fabricated data in CI. |
| **Experiments** (`routes/experiments/**`) | n/a | **no seam yet** | Nothing. Hypothesis, baseline, readings, horizon, verdict - no contract, no store. |
| **Global search** (`lib/search-api.ts`) | n/a | **no seam yet** | The global search endpoint spans seven kinds (`todo, session, note, employee, cron, skill, page`) with facet parsing and highlighted snippets. Four of those seven kinds have no seam at all, and no seam exposes a query surface - every harness read is a list or a get. The most cross-cutting gap in the table. |
| **Auth** (`routes/auth-provider.tsx`, `AuthGate`, `lib/auth.ts`) | n/a | **no seam yet** | Named explicitly in `README.md`: there is no authentication or authorization; loopback plus the port the `jinn:net` grant scopes is the entire boundary. No token, no bearer, no per-route authority. The web app's `AuthGate` has nothing to gate against. |

Two structural gaps sit under the whole table rather than in one row: **no way to serve the
bundle** (no static assets, and `jinn:net` v0.1 binds loopback TCP only), and **no push
transport** (every feed is a cursor because of #4/#32; latency compounds per layer, measured
513/755/1084 ms at two/three/four layers, #35).

### 4.3 Interception moments

Naming scheme: `<domain>:<before|after|on>-<noun>`. `before-` is a waterfall (the listener
returns a modified value); `after-` is a waterfall over the value on its way *out*; `on-` is a
notification. Ordered by how likely the operator is to reach for them.

Every gateway-side moment inherits the two caveats from 4.0: a listener whose incarnation owes
a transition **refuses the whole walk** (nothing is delivered), and a listener that calls back
into the emitting seam **deadlocks to the guest deadline** (#4/#32). Both are properties of the
pin, not of the design.

**1. `chat:before-send`** - the operator's own example.
*Where:* `components/chat/chat-input.tsx` → `sendText(rawText, media)` (the single choke point for both Enter/tap via `handleSubmit` and STT auto-send via `applyTranscript`), and one layer down `components/chat/chat-pane.tsx` → `handleSend(message, media, interrupt, speech)`. Gateway twin: `jinn-session`'s `OP_SEND` with `SendRequest { session-id, message }`.
*Receives / returns:* `{ text, attachments, sessionId, engine, model, effort, speech }` → a modified `text` and/or `attachments`. Refusal (`{ cancel: reason }`) should also be expressible, since a validator is the second-most-obvious extension.
*Side:* **both.** Client-side gets the composer state and instant feedback; gateway-side is the only place that also catches a send arriving from cron, a workflow node, or the API.
*Extension:* "append an emoji"; realistically also "expand a shorthand into my standing brief", "refuse a send containing an API key", "prefix every prompt with the repo I'm in".

**2. `chat:after-render-message`**
*Where:* `components/chat/message-markdown.tsx` → `formatMessage` / `inlineFormat` (a hand-rolled regex formatter, not a library), grouped by `components/chat/chat-messages.tsx` → `groupMessages`.
*Receives / returns:* the message's text plus its parsed inline nodes → modified nodes, or an extra decorator node. A waterfall over *nodes* rather than over the string is what keeps two extensions composable.
*Side:* **client-only** - nothing gateway-side owns presentation.
*Extension:* "linkify our internal ticket ids"; "collapse anything over 40 lines behind a fold".

**3. `chat:before-create-session`**
*Where:* `components/chat/new-chat-helpers.ts` → `buildNewSessionParams(...)`, consumed at `chat-pane.tsx` `handleSend` before `api.createSession`. Gateway twin: `jinn-session` `OP_CREATE` with `CreateRequest { spec: SessionSpec }`.
*Receives / returns:* the whole `SessionSpec` - `engine: EngineBinding`, `cwd`, `tools: ToolPolicy`, `attribution`, `metadata` → a modified spec.
*Side:* **both**, and the highest-leverage gateway-side moment in the list, because `SessionSpec` is where the engine, the cwd and the tool policy are all decided at once.
*Extension:* "any session I start from a repo directory gets that repo as cwd and the repo's tool policy".

**4. `engine:before-run`**
*Where:* no direct web-side equivalent - the selector state is `components/chat/model-selector-row.tsx` with `handleSelectorChange` in `chat-pane.tsx`, persisted through `api.updateSession`. Gateway: `jinn-engine`'s `OP_RUN`, `RunRequest { engine, model, effort, prompt, cwd, tools, budget, secrets }`.
*Receives / returns:* the `RunRequest` → a modified `model`, `effort`, `budget`, or `prompt`.
*Side:* **gateway-only.** It is below the session, so it catches every caller.
*Extension:* "route anything under 200 characters to the cheap model, and cap the budget on anything a cron fired".

**5. `chat:on-stream-delta`**
*Where:* `hooks/use-live-session.ts` → the single `subscribe()` effect that switches on `frame.event` (`session:started`, `session:delta` of type `text`/`text_snapshot`/`tool_use`/`tool_result`/`block`/`context`/`status`, `session:notification`, `session:attachment`, `session:interrupted`/`stopped`/`completed`, `session:background`, `session:external-turn`). Gateway: `jinn:session/event`.
*Receives / returns:* one event frame → a modified or suppressed frame. Honest naming matters here: on the harness side this is a **cursor read**, so a gateway-side variant is `chat:after-read-events` over a page, not a per-delta hook.
*Side:* **both**, with different shapes on each side.
*Extension:* "mute tool-noise deltas and show me one line per tool group".

**6. `todo:before-status-change`**
*Where:* `routes/todos/todo-status-mutation.ts` (optimistic cache write) and `lib/legal-targets.ts` (`legalTargets`, the client-side legality pre-check that mirrors the gateway). Gateway: `Todos::plan_update` / `Status::transition`.
*Receives / returns:* `{ todoId, from, to, actor, note }` → a modified target status, an added note, or a refusal. The seam already answers `Moved::Refused` carrying its own record, so a listener refusal has a place to land without inventing one.
*Side:* **both.** Client for the pre-check, gateway for the authority.
*Extension:* "when anything moves to `in-review`, auto-comment the branch name and assign my reviewer".

**7. `chat:before-attach`**
*Where:* `components/chat/chat-input.tsx` → `fileToAttachment` / `resizeImage`, reached from `handleFileAttach`, `handlePaste` and the drop path; upload happens later at send time via `api.uploadFile`.
*Receives / returns:* `{ file, mime, bytes, sessionId }` → a replaced or rejected attachment.
*Side:* **client-only today** (there is no file seam on the target), which is worth saying out loud: this moment cannot be ported until one exists.
*Extension:* "strip EXIF from every screenshot before it leaves the machine".

**8. `nav:after-build-tree`** - the moment that makes "the UI is a renderer of a plugin tree" true rather than aspirational.
*Where:* `lib/nav.ts` → `contributedNavItems()` merged with `BASE_NAV_ITEMS`.
*Receives / returns:* the ordered nav item list → reordered, filtered, relabelled, or extended.
*Side:* **client-side**, sourced from a gateway-side plugin catalog read.
*Extension:* "hide Experiments and Limits, put Todos first, rename Chat to the project I'm in".

**9. `theme:after-resolve-tokens`**
*Where:* `hooks/use-root-css-variables.ts` → `useRootCssVariables(settings)`, which publishes `--accent`, `--accent-fill`, `--accent-contrast`, `--text-scale` onto the root element; palette ids in `lib/themes.ts`.
*Receives / returns:* the resolved token map → a modified map.
*Side:* **client-only.**
*Extension:* "tint the whole shell red while a production cron is mid-run".

**10. `notify:before-show`**
*Where:* `plugins/sdk/plugin-notices.tsx` (the one sink, `registerHostNotificationSink`, max 3 visible, 6000ms auto-dismiss) and the `session:notification` branch in `use-live-session.ts`.
*Receives / returns:* `{ level, title, body, source }` → a modified, rerouted, or suppressed notice.
*Side:* **both** (a gateway-side variant could reroute to a connector, once one can exist).
*Extension:* "anything at `error` level also goes to my phone; anything from the health-snapshot job is silent".

**11. `workflow:before-node-run`**
*Where:* `routes/workflow/run-inspector.tsx` / `run-canvas.tsx` render it; the authority is the node-state transition table in `jinn-workflow` and the node-state route.
*Receives / returns:* `{ runId, nodeId, spec, input, definitionRevision }` → a modified node input, or a skip.
*Side:* **gateway-only.** Note the constraint that makes this delicate: a run is pinned to one revision for its whole life, so a listener that rewrote a node's *spec* would break the pin's guarantee. Only the **input** should be waterfall-able.
*Extension:* "inject the current sprint id into every dispatch node's prompt".

**12. `cron:before-fire`**
*Where:* `routes/cron/detail.tsx` and `run-row.tsx` display it; the authority is `cron-scheduler`'s per-wake plan and its `FirePayload` emit.
*Receives / returns:* `{ jobId, scheduledMs, payload }` → a modified payload, or a suppressed fire.
*Side:* **gateway-only.**
*Extension:* "skip the morning digest on days I'm away"; "add today's date to every fire payload".

**13. `tool:before-invoke`**
*Where:* nothing intercepts it in the web app today - tool calls only *appear*, as synthetic rows rendered by `components/chat/tool-group.tsx`. The decision point is `ToolPolicy` inside `SessionSpec` / `RunRequest`, which is default-deny (an absent policy admits no tool).
*Receives / returns:* `{ sessionId, tool, args }` → modified args, or a deny.
*Side:* **gateway-only**, and honestly: this one needs the engine providers to surface a per-call decision point they do not have today - `jinn-engine-claude` and `-codex` spawn a CLI and read its stream; the policy is handed to the child, not consulted per call. **Flagged as the moment with the largest gap between how obvious it is and how buildable it is at this pin.**
*Extension:* "never let a session run a destructive command outside a worktree".

**14. `tool:after-result`**
*Where:* the `tool_result` branch of the `subscribe()` switch in `hooks/use-live-session.ts`, rendered by `tool-group.tsx` → `ToolGroup`.
*Receives / returns:* `{ tool, result, durationMs }` → a modified or summarised result.
*Side:* **both.**
*Extension:* "truncate any tool result over 200 lines to its first and last ten".

**15. `search:after-results`**
*Where:* `lib/search-api.ts` (the client half of the global search endpoint; kinds `todo, session, note, employee, cron, skill, page` in a fixed presentation order).
*Receives / returns:* the grouped result set → reordered, filtered, or extended with a synthetic group.
*Side:* **both** - but there is **no search seam at all** on the target, so this is a moment for a capability that has to be built first.
*Extension:* "put my own project's tickets above everything else, and add a group that searches my local scratch directory".

**16. `search:before-query`**
*Where:* the facet parser implied by `QueryFacetWire` / `QuerySpanWire` in `lib/search-api.ts` (the gateway owns the parse; the client echoes the spans back to render removable chips).
*Receives / returns:* `{ raw, facets, spans }` → an added or rewritten facet.
*Side:* **gateway-side**, same caveat as 15.
*Extension:* "`mine` always expands to assignee:me status:!done".

**17. `todo:before-create`**
*Where:* `routes/todos/new-todo-dialog.tsx` and `lib/api-todo-capture.ts`; gateway authority is `jinn-todo`'s `create`.
*Receives / returns:* the Todo spec (title, body, parentId, labels) → a modified spec.
*Side:* **both.**
*Extension:* "every Todo I create from chat gets the session id as a comment and inherits the open board's prefix".

**18. `session:on-lifecycle`**
*Where:* create at `chat-pane.tsx` `handleSend`; resume at `use-live-session.ts` `loadSession`; stop at `handleInterrupt` → `api.stopSession` and the standalone `useStopSession` in `hooks/use-sessions.ts`; archive/delete/duplicate alongside it. Gateway: the session status/turn state machine in `jinn-session`.
*Receives / returns:* `{ sessionId, from, to, reason }` → notification, not waterfall. Naming it `on-` rather than `before-` is deliberate: the seam's whole honesty discipline is that `running` is minted only by the live registry and a replayed turn cannot claim it, so a listener must not be able to author a status.
*Side:* **both.**
*Extension:* "when a session ends `interrupted`, open a Todo with its last turn quoted".

**19. `settings:before-patch`**
*Where:* `routes/settings/page.tsx` with `config-shape.ts` and `config-conflict-notice.tsx`; gateway: `jinn-settings`'s patch plan and the `changed`/`refused` payloads, over `PATCH /v1/settings/{ns}`.
*Receives / returns:* `{ namespace, patch, layer }` → a modified patch, or a refusal.
*Side:* **both.** This is the one moment where a waterfall already has a native shape - the seam emits `Serial` on the hot path today (`plugins/settings/jinn-settings-profile/src/lib.rs:278`), so moving it to `waterfall` is a mode change rather than new plumbing.
*Extension:* "never let anything set my accent colour outside my three approved values".

**20. `approval:before-decide`**
*Where:* `lib/work-item-approval-wire.ts` (`ApprovalStateWire`, `WorkItemApprovalWire` with `options` for a pick), the banner in `routes/todos/task-page/banner.tsx`, and `routes/workflow/approval-decision.tsx`.
*Receives / returns:* `{ approvalId, request, options, decision }` → a modified decision or an added justification.
*Side:* **both** - **no approvals seam exists on the target**, so this is aspirational. Named because approvals are the one place a bad extension does real damage, and the design should decide *now* whether interception is even allowed there. The read from here: it should not be waterfall-able. An extension may *observe* an approval and may *pre-fill* a justification; it must never be able to change a decision.
*Extension (observing only):* "post every pending approval to my phone with its options".

**21. `plugintree:after-list`**
*Where:* `routes/settings/plugins/page.tsx` renders the enabled set; `plugins/disk-plugins.ts` reconciles it. Gateway: `jinn-plugins`'s `list`/`describe`.
*Receives / returns:* the catalog rows → reordered, annotated, filtered.
*Side:* **both.**
*Caveat, and it is a sharp one:* `FINDINGS.md` #41 says three of eleven lifecycle readings are unreachable at this pin, and a measured restart was invisible across 189 consecutive reads. **An extension that watches a plugin's life through this surface will silently miss every transition.** A `plugintree:on-lifecycle` moment should *not* be offered until the kernel gains a publish path (carded M2-K13, #40).
*Extension:* "group my own extensions above the first-party ones and show me which ones failed".

**22. `logs:after-read`**
*Where:* `routes/logs/page.tsx`. Gateway: `GET /v1/ledger/tail` via `jinn-status`.
*Receives / returns:* the ledger page → filtered or annotated lines.
*Side:* **both.**
*Caveat:* the ledger caps at 500 lines above any window, and reading the plugins catalog itself costs ledger lines (three ledgered contract calls per answer) - an extension that polls this surface grows the thing it is reading.
*Extension:* "hide every dispatch trace and show me only refusals".

### 4.4 What "the UI is a renderer of a plugin tree" costs

The web app already has the *shape* of the answer, and it is honest about how far it gets. The
v1 contribution areas are exactly seven, declared once and frozen
(`packages/web/src/plugins/sdk/areas.ts`, mirrored in `packages/web/src/contrib/types.ts`):
`routes`, `sidebar.nav`, `statusbar.right`, `todo.detail.actions`, `todo.detail.sections`,
`chat.composer`, `home.widgets`. A plugin can own a whole page - `ContributedRoute` mounts on
the router's *last* splat child, so shadowing a core route is structurally impossible rather
than merely discouraged.

**Cheap to become plugin entries:** notes, skills, experiments, limits, logs, file - each is
one route, a list and a detail, and no shell state. `/redesign` and `/talk-orb` already read as
opt-in surfaces. Cron and org are nearly as cheap once their seams exist.

**Load-bearing shell that stays core:** the provider stack in `routes/client-providers.tsx`:
QueryClient → Theme → Auth → AuthGate → Settings → Gateway - plus the router in `main.tsx` and
the one socket in `hooks/use-gateway.tsx`. Nav is the interesting boundary: it is core code
that *merges* contributions (`lib/nav.ts`), which is the right split.

**Where today's disk-plugin system falls short, stated plainly.** Its own loader says it: "this
is error isolation, not a capability boundary… evaluated as ESM in the dashboard's own realm
with the app's full authority. It cannot crash the app; it can do anything the app can"
(`packages/web/src/plugins/runtime-loader.ts` header). Enablement lives in config, not in the
plugin. The host-verb door is a fixed 16-verb union with every verb granted
(`plugins/sdk/host-permissions.ts` - "v1 grants every verb, so nothing is refused today"). And
the decisive gap: **events are `void`** - inbound gateway frames only, fire-and-forget, no
return channel.

So porting buys three things the current system cannot have: a real capability boundary (WASM
plus the kernel's grant check, instead of same-realm ESM), a per-verb grant that can actually
refuse, and - the one the operator asked for - a return channel, because `waterfall` and
`handle-event -> result<list<u8>>` are already in the pinned WIT.

**What has to exist in the harness before a plugin contributes a real surface:** (a) a way to
*serve* the bundle - there is no static-file path and `jinn:net` v0.1 binds loopback TCP only;
(b) a push, or at minimum a long-lived read - no keep-alive, no SSE, no WebSocket, and every
feed is a cursor because of #4/#32; (c) an area/contribution contract as a typed seam, so a UI
entry is a profile entry; (d) auth, which does not exist at all today.

### 4.5 Ordering

Derived from seam fit, not preference.

1. **Settings** - the only `direct` row. Namespaces, schemas, layered patches and typed refusals are all there, and it is also where `waterfall` is one mode change away from being real.
2. **A UI-hosting transport, before any screen moves** - no static serving, no keep-alive, and no push exist today; every surface below is blocked on this, so it goes first even though it is not a "surface".
3. **Plugin tree / settings-plugins page** - read-only, three ops, and its `list`/`describe` already exist. Ship it *without* any lifecycle-watching feature, per #41's measurement.
4. **Todos** - `partial`, but the gap is enumerable: reconcile the two status vocabularies first (8-with-reopen vs 6-terminal), then approvals, labels and versions. Do not port the board until the vocabulary is one thing.
5. **Workflows** - the seam is deep and honest; the missing pieces (triggers, enable/disable/retire/duplicate, node retry, run approvals) are additive routes on an existing contract rather than a new capability.
6. **Chat** - the highest-value surface and deliberately *not* first: it needs the push transport from step 2, plus attachments, blocks, employees and interrupt, none of which have a seam.
7. **Cron** - cheap once someone writes the routes; today it is a capability with no wire.
8. **Logs and limits** - thin adapters over the ledger tail and per-run budgets; both will read short until the ledger window and the aggregate-usage question are answered.
9. **Org/employees, notes, skills, files, experiments, global search, auth** - each waits on a seam that does not exist. Search waits on four of them at once and should be last of these; **auth should be first of them**, because every surface above ships today behind nothing but a loopback port.

### 4.6 Stated uncertainty

- **Waterfall has never been exercised in this repo.** The mode is in the pinned WIT and the return channel is in `handle-event`; whether a JS-authored listener inside a WASM guest performs acceptably in a send path is unmeasured, and #35's per-layer latency numbers are for polling, not for listener walks.
- **`tool:before-invoke` (moment 13)** may not be buildable at this pin at all - the engine providers hand a policy to a spawned CLI rather than consulting one per call. Not read deeply enough to say whether a per-call decision point could be added without a kernel change.
- The workflow *trigger* implementation on the source side was not read, so "no trigger counterpart" in 4.2 is an inference from the harness side's silence, not from a reading of both.
## 5. The pin

Everything in this document describes the source repo at exactly:

| | |
|---|---|
| **Commit** | `43e864750168e163b55855a79f955e471da0bcc1` |
| **Short** | `43e8647` |
| **Branch** | `main` |
| **Dated** | 2026-08-30 21:49:33 +0300 |
| **Subject** | `Merge remote-tracking branch 'origin/main'` |
| **Working tree at survey time** | clean except one untracked design spec under `docs/superpowers/specs/`, which no section reads |
| **Surveyed** | 2026-09-01 |

Cross-referenced against this repo at `e6a7935` (`Merge pull request #15 from
packet/2.7-plugins-ux`), kernel pin `3a8e5c0`.

**Why the pin is named rather than assumed.** The web UI is still moving - focus-pills and
ledger redesigns are in flight on that repo - and that is accepted. Naming the snapshot is what
makes a later re-sync a readable diff rather than a surprise: `git diff 43e8647..<new> --
packages/web` answers "what changed since the inventory" directly. Without it, the next reader
has to re-derive the whole survey to find out whether it is still true.

**Line counts in this document** are measured at that commit. Commit shas cited in section 2
are ancestors of it and remain valid references regardless of what lands later.

**How to re-sync.** Diff `packages/web` from this sha forward. Section 1's route table and
section 2's file references are the two places that go stale first; sections 3, 4 and 6 describe
contracts and are more durable. Re-run the archaeology only for files the diff actually touched.

## 6. Toolchain and gate implications

What the TypeScript app needs in `jinn-harness` that is not there today. All counts
are measured at the pinned sha, not estimated.

### 6.1 What the web package actually is

| Fact | Value |
|---|---|
| Source | `packages/web/` - 147,164 lines of `.ts`/`.tsx` |
| Of which tests | 399 files, 61,064 lines (~41% of the tree) |
| Runtime deps | 30 |
| Dev deps | 19 |
| Static assets | `public/` 216 KB (icons, `manifest.webmanifest`, `sw-shell-warm.js`), `src/fonts/` 144 KB woff2 |
| Build output | `packages/web/out/` (not `dist/`) |

The heavy runtime deps are not incidental - each one is a surface: `@xyflow/react`
+ `@dagrejs/dagre` + `d3-hierarchy` (workflow canvas and org map), `@tiptap/*` +
`tiptap-markdown` (the composer), `@xterm/*` (the CLI terminal), `react-markdown`
+ `remark-gfm` + `react-syntax-highlighter` (the one markdown renderer),
`radix-ui` + `cmdk` + `lucide-react` (primitives, command palette, glyphs),
`@tanstack/react-query` + `@tanstack/react-virtual` + `zustand` (state and
virtualization), `react-router-dom@7`, `vite-plugin-pwa`, `emojilib`.
Dropping any of them drops the surface that uses it.

### 6.2 Build toolchain to add to a Rust repo

The harness has `rust-toolchain.toml` and nothing for Node. The port needs:

- **Package manager and Node pin.** The source repo pins `pnpm@10.6.4` via
  `packageManager`, and pins Node itself in `.npmrc` with `use-node-version=24.13.0`
  plus `engine-strict=true`. That pin is not cosmetic - its comment records the
  incident it prevents (a Homebrew Node of a different ABI silently recompiling a
  native module and crashing the daemon). The harness inherits the pin discipline
  it already applies to the kernel; it should inherit this one too.
- **Vite 7** + `@vitejs/plugin-react-swc`, **Tailwind 4** through
  `@tailwindcss/postcss` + `postcss`, **TypeScript 5.8**, **Vitest 4** + **jsdom 29**,
  **ESLint 10** + `typescript-eslint@8`.
- **Turbo is not needed.** The source repo uses it to orchestrate four packages; one
  TS package can be driven by plain pnpm scripts. One fewer moving part.
- **A wasm32 target is already in CI** (the `composition-gate` job), so the
  JS-in-WASM extension tier does not add a new toolchain requirement, only a build
  step.

### 6.3 `.gitignore` - the firewall gap

The harness `.gitignore` currently contains exactly one line, `target/`, with a
comment stating the bound: build state never enters history because target trees
carry machine paths in their metadata, and CI enforces it. A JS tree brings the
same class of hazard in four more forms. Before the first TS file lands:

```
node_modules/
dist/
out/
coverage/
*.tsbuildinfo
.turbo/
test-results/
playwright-report/
```

`out/` and `*.tsbuildinfo` matter most: `packages/web/out` is the vite build
directory and `tsconfig.json` writes `./tsconfig.tsbuildinfo` beside the source, so
an incremental typecheck drops a machine-path-bearing file into the tree on the
first local run. Neither is covered by a `dist/` rule.

### 6.4 The CI privacy firewall needs widening, and it will bite

The harness gate is two greps:

```
git ls-files | grep -E '(^|/)target/'          -> must be empty
git grep -I -n -E '/(Users|home)/[A-Za-z]'     -> must be empty (excludes ci.yml)
```

Three consequences for a JS tree:

1. The `target/` check must gain `node_modules/`, `out/`, `dist/` - otherwise the
   check is a rule that documents more than it enforces.
2. The second half of that path pattern (the Linux home prefix) will fire on ordinary
   CI text: a runner path in an Actions snippet, or any doc that quotes one, is
   tracked content. The exclusion list has to grow, or the pattern has to narrow,
   deliberately and with a comment, before the first Node lane is added. This very
   document had to be worded around it.
3. It is `git grep -I` over tracked content only, so as long as (1) holds, build
   output is out of scope by construction. That is the right shape; keep it.

### 6.5 The two gates in the source repo, and whether they come across

Both are decisions, not defaults.

**`scripts/check-footguns.mjs`** - a diff-scoped checker whose header states the
design: every rule is an incident that actually happened, and it judges only the
lines a change adds, so existing debt never blocks a merge and the check is never
red on arrival. Its `personal-path` rule is the same bound the harness firewall
already enforces, so the two overlap rather than conflict. Its other rules
(hardcoded home, production-port, env reads drifting from the config layer, unread
child pipes) are gateway-shaped and mostly do not apply to a view layer - but the
mechanism (diff-scoped, inline `// footgun: ok <reason>` suppression that lists
unaudited suppressions) is worth carrying whatever the rule set ends up being.

**`scripts/ratchet.mjs`** - a 300-line-per-file cap with `size-baseline.json` as the
record of pre-existing debt, budgets that may only shrink, and a mirror of the
ESLint grandfather list so the two cannot drift. `packages/web/` is one of its
scanned trees. **The port's arithmetic:** 153 of the baseline's 365 entries are web
files; 50 of those carry a line budget, totalling 30,759 lines. The largest are
`hooks/__tests__/use-live-session.test.ts` (2,103), `components/chat/chat-sidebar.tsx`
(1,767), `hooks/use-live-session.ts` (1,428), `routes/settings/page.tsx` (1,427),
`components/chat/chat-messages.tsx` (1,334), `lib/api.ts` (1,075),
`components/chat/chat-input.tsx` (1,046), `routes/chat/page.tsx` (996).

So there are exactly three options, and they should be chosen rather than drifted
into: carry the ratchet **and** the 153-entry baseline (the tree arrives green and
the debt stays visible and shrinking); carry the ratchet **without** the baseline
(50 files fail on day one - porting verbatim and splitting 30k lines are the same
packet, which the brief explicitly does not want); or leave the TS tree ungated for
size. Recommendation, one line: carry both, since a baseline is a diff a reviewer
sees and an ungated tree is not.

### 6.6 Cross-package dependencies that leave the repo boundary

The web app does not compile against itself alone. `packages/web/tsconfig.json`
declares four aliases that reach outside the package:

| Alias | Resolves to | Source LOC | Web files importing it |
|---|---|---|---|
| `@jinn/gateway-events` | workspace package `packages/gateway-events` | 475 (+62 test) | 35 |
| `@jinn/fallback-map-wire` | `packages/jinn/src/shared/fallback-map-wire.ts` | 103 | 3 |
| `@jinn/workflow-wire` | `packages/jinn/src/workflows/wire.ts` | 89 | 1 |
| `@jinn/model-id` | `packages/jinn/src/shared/model-id.ts` | 21 | 1 |
| `@jinn/plugin-sdk` | internal, `src/plugins/sdk/index.ts` | n/a | 10 |

The last three are small and local (213 lines across 5 consuming files) - port or
re-express, either is cheap. **`@jinn/gateway-events` is the one that matters**: it
is the realtime event vocabulary, it is consumed by 35 files, and it is the exact
place where the old gateway's event model meets the view layer. The harness has its
own event bus and its own contract surface, so this is a decision point rather than
a copy: either the package ports verbatim as the client's view of the bus, or the
harness's event names become the client's vocabulary and 35 files change. The
coupling report (§3) should be read before that choice is made.

A second-order note the port must not miss: `turbo.json` carries a comment
explaining that type-aware lint reads `@jinn/gateway-events`'s **emitted
declarations**, and without them `act(() => emit(...))` in the web tests widens to
a thenable and eleven phantom `no-floating-promises` errors appear. If that package
becomes a plain source import rather than a built package, that lint failure comes
back.

### 6.7 The build-output contract the new host must reproduce

`scripts/sync-web-dist.mjs` copies `packages/web/out` into the gateway's
`dist/web`, and it does two specific things that are not incidental:

- it asserts every `/assets/...` reference in `index.html` exists on disk before
  copying, failing the build if one is missing;
- it copies **everything except `index.html` first**, then swaps `index.html` in
  last through a uniquely-stamped temp file.

That ordering is the whole point: the new assets are on disk before any index
references them, so a client fetching mid-deploy never gets an index pointing at a
chunk that is not there yet. A content-addressed pinned artifact gets this property
for free **if** the pin flips atomically and old chunks are not deleted out from
under a client that already loaded an old index. If the serving plugin deletes the
previous artifact on pin flip, the stale-chunk class of failure returns in a new
costume. (§2 has the client-side half of this fix; read them together.)

### 6.8 Things that live outside `packages/web` and will be left behind

- **Playwright e2e** - `playwright.config.ts` and `e2e/` sit at the source repo
  root, not in the web package: `scroll.spec.ts`, `smoke.spec.ts`, plus the
  `workflow-layout` and `chat-grid-drop` harnesses. Porting the package alone
  silently drops them.
- **The vitest flaky-retry wrapper** - `packages/web/package.json`'s `test` script
  is `node ../../scripts/vitest-flaky-retry.mjs`, a root script. That relative path
  breaks the moment the package moves; the wrapper itself exists for a reason worth
  recovering before it is discarded.
- **The perf budget** - `packages/web/scripts/perf-budget.mjs` + `perf-budgets.json`
  are in-package and come across, but nothing currently runs `perf:budget` in CI.

### 6.9 CI lane to add

One job, gated the way the Rust lanes already are:

```
pnpm install --frozen-lockfile
pnpm --filter @jinn/web typecheck      # tsc --noEmit
pnpm --filter @jinn/web lint           # eslint
pnpm --filter @jinn/web test           # vitest (399 files)
pnpm --filter @jinn/web build          # vite build
```

plus a pnpm-store cache step alongside the existing `Swatinem/rust-cache`, and the
widened privacy-firewall step from §6.4 running before any of it.

### 6.10 Open decisions this section does not resolve

1. Where the TS tree lives in a Cargo workspace - a top-level `web/`, or under the
   serving plugin. Guest plugin crates are deliberately not workspace members
   (`Cargo.toml` says so); the TS package is not a Cargo member at all, so it needs
   a stated home rather than an inferred one.
2. Whether the built bundle is vendored into the repo as an artifact or built in
   CI and pinned by hash. This decides whether `.gitignore` excludes `out/` or
   whether a hashed artifact directory is tracked deliberately.
3. Whether the service worker survives. `vite-plugin-pwa` + `public/sw-shell-warm.js`
   are a client-side caching layer sitting directly on top of the artifact-pinning
   story; the two need one owner, not two.
4. The `@jinn/gateway-events` question in §6.6.
5. Whether the ratchet and footgun gates come across (§6.5).
