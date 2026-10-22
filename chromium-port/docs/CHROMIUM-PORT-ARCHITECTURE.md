# Zen Browser → Chromium Port: Platform & Build Architecture

Scope: the platform decision — what we fork, how the build/patch machinery works,
how Brave's ad-blocker is wired into Chromium, where the Zen UI work would land,
and the risks. UI feature parity (vertical tabs, workspaces, split view, glance,
compact mode) is implemented by other agents; this document only defines the
ground they build on.

Sources are primary: `brave/brave-core` source tree, `brave/adblock-rust`,
`brave/brave-browser` wiki (cloned), Chromium docs. Facts below were verified
against the repos, not inferred.

---

## 1. Base platform options

### Option A — Fork `brave/brave-core` (Chromium + Brave components). **Recommended.**

`brave-core` is the entire Brave browser *minus* Chromium: it mounts at
`src/brave` inside a depot_tools/gclient checkout of `brave/chromium` (a mirror
of `chromium/src` pinned by tag). It carries the build tooling, the patch
system, the ad-block stack, and — critically for us — most of Zen's UI surface
already implemented:

| Zen feature | Already in brave-core? |
|---|---|
| Vertical tabs | Yes — `browser/ui/views/frame/vertical_tabs/` (`VerticalTabStripRegionView`), `browser/ui/tabs/vertical_tab_controller.cc`, plus chromium_src overrides of upstream `chrome/browser/ui/tabs/vertical_tab_strip_state_controller*` (upstream Chromium has also been landing vertical-tab work; Brave extends it). |
| Split view | Yes — `browser/ui/split_view/` + `browser/ui/views/frame/split_view/` (`BraveMultiContentsView`, `BraveContentsContainerView`) built on upstream `MultiContentsView`. |
| Workspaces | Mostly — `browser/ui/views/workspaces/` (bubble, row view, save dialog) on top of Chromium's workspaces feature. |
| Tree tabs / folders | Yes — `browser/ui/tabs/tree_tab_model.*`, `components/tabs/impl/tree_tab_node_tab_collection.*`. |
| Containers | Yes — `browser/containers/` (Firefox-style containers service + storage partitions). |
| Glance / compact mode | No direct equivalent — new work (see §4). |

**Pros**
- Ad-block arrives "for free": not just the engine, but the request-interception
  plumbing, cosmetic filtering, CNAME uncloaking, filter-list distribution
  (Component Updater + regional catalog), subscription manager, scriptlet/redirect
  resource library, element picker, devtools instrumentation, and the Shields UI.
- The patch/override machinery (`chromium_src/`, `patches/`, `rewrite/` plaster)
  is designed to keep a Chromium fork rebaseable — our delta rides the same rails.
- License-compatible end to end: `brave-core` is **MPL-2.0** (verified in
  `LICENSE`), `adblock-rust` is MPL-2.0, `adblock-cxx` is MPL-2.0, Zen is MPL-2.0,
  Chromium is BSD-3 — all mutually compatible; our modifications stay MPL-2.0.
- Hermetic toolchains and a documented rebase process (§5) lower CI/QA cost.

**Cons**
- Ships Brave features we don't want (Rewards/crypto, Wallet, VPN, Leo AI, News,
  Talk, Web3). Mitigation: these live behind buildflags/GN args and pref
  defaults; we strip or disable them rather than fork them out. Some removal is
  invasive — budget for it.
- Brave service endpoints are baked in (filter-list component hosting,
  `go-updater.brave.com`, `brave-core-ext.s3.brave.com`, `brave_domains/`).
  We either (short-term) keep pointing at Brave's endpoints — needs a ToS/legal
  sanity check — or run our own component-update + extension-update server.
- Rebranding cost: `brave://*` WebUI, `brave_strings.grd`, icons, `chrome://brave-*`
  resources, product name threading through ~every layer.

### Option B — Fork vanilla Chromium + integrate only `brave/adblock-rust`

`adblock-rust` is a standalone crates.io crate (`adblock`, currently v0.13,
MPL-2.0) with a `cxx` bridge (`adblock-cxx`). It can be linked into any Chromium
build via `rust_static_library` / cargo_crate GN targets — Chromium itself now
has first-class Rust support (`//build/rust`), and Brave vendors the crate under
`third_party/rust/adblock/v0_13` via `gnrt`.

The engine is the *small* part. What you must reimplement to reach Brave parity:

- **Request interception**: a browser-process `URLLoaderFactory` proxy
  (`BraveProxyingURLLoaderFactory` in Brave) wired through
  `*ContentBrowserClient::WillCreateURLLoaderFactory`, plus WebSocket/WebTransport
  hooks, plus per-request context (`BraveRequestInfo`: tab, initiator origin,
  resource type).
- **Engine lifecycle**: per-profile engine(s) on a dedicated sequence,
  serialized `.dat` cache of the compiled filter graph, regional/subscription/
  custom list provider managers.
- **Cosmetic filtering**: renderer-side injection of CSS at `document_start`,
  procedural filters, hidden class/id selector callbacks — a browser↔renderer
  mojom channel and a `RenderFrameObserver`.
- **Filter-list distribution**: packaging lists as Component Updater CRXs needs
  a `brave-core-crx-packager`-equivalent and an update server, or an extension-
  update-protocol host; plus the regional list catalog.
- **Everything else Chromium doesn't give you**: tab strip, toolbar, omnibox
  behavior, sessions, downloads UI — i.e., the entire browser chrome that
  brave-core (and our UI agents' work) needs anyway.

In effect option B = "re-derive brave-core, minus the Brave brand, plus all our
own patch infrastructure." The upstream-Chromium patch burden is *worse* than
option A because we own 100% of the delta and have no `chromium_src`/plaster
machinery. Only choose this if Brave code were license-incompatible or the Brave
surface were deemed unremovable — neither is true.

### Option C — Wrappers (Electron / CEF / Tauri)

- **Electron**: `webRequest` can block/redirect at the network level from the
  main process, but it's an async, per-request IPC hop — meaningfully worse
  latency/CPU than in-process matching — and cosmetic filtering needs hand-rolled
  per-frame CSS injection. More decisively: Electron provides *no browser
  chrome* — tabs, omnibox, bookmarks, sessions, sync, autofill, devtools,
  downloads are all rebuilt in HTML. That is a full browser reimplementation on
  top of a thinner platform, and Brave-style performance characteristics
  (in-engine matching, serialized filter cache) are unreachable.
- **CEF**: closer — `CefResourceRequestHandler` can block/rewrite requests — and
  it is real Chromium. But CEF is a library, not a browser: again the whole
  chrome layer, plus lifecycle glue (multi-process bootstrap, printing, PDF,
  devtools, extensions support is partial/absent). Same "rebuild a browser to
  rebuild a browser" problem, worse tooling than Chromium's own build.
- **Tauri**: uses the *system* webview (WebView2 / WKWebView / WebKitGTK). Not
  Chromium, per-platform engines, no unified extension/declarative-net-request
  story, no `URLLoader` surface to plug adblock-rust into. Wrong substrate.

All three also lose: real `chrome://*` settings infra, the extension ecosystem
contract (MV3), devtools, safe-browsing plumbing, autofill/passwords — i.e.,
they solve "embed a webview," not "ship a browser."

### Recommendation

**Fork `brave/brave-core` as `src/brave` inside the Brave-managed Chromium
checkout, and build Zen as a layer on top of Brave** — new `zen/` code +
`chromium_src` overrides + `patches`/`rewrite` plaster, using the same machinery
Brave uses for its own delta. Strip unwanted Brave features via buildflags/GN
args; rebrand strings/resources/WebUI; point `brave_domains` and the component
updater at our own endpoints (or Brave's, pending legal review).

Rationale: the ad-blocker is not a library to be bolted on — it is an engine
plus five subsystems of plumbing that Brave already built, maintains, and
rebases every ~2 weeks. On top of that, Brave already implements or extends 4 of
the 5 headline Zen UI features (vertical tabs, split view, workspaces, tree
tabs/containers), so the UI agents start from working platform code instead of
upstream-Chromium stubs.

---

## 2. How the Brave patch system works (mechanics)

### Repo layout

- `brave/brave-browser` — **not** the code. Issues, releases, wiki only.
- `brave/brave-core` — all browser code + build tooling. Cloned to
  `<proj>/src/brave` inside a gclient checkout. On master this is ~60k+ files:
  `browser/` (≈4.3k), `components/` (≈11.5k), `chromium_src/` (≈3.0k),
  `patches/` (≈1.3k `.patch` files), `rewrite/` (≈1.6k plaster YAML),
  `ios/`, `android/`, `third_party/` (incl. vendored Rust crates), `build/`,
  `script/`, `tools/`, `docs/` (excellent internal docs — see `docs/` list).
- `brave/chromium` — mirror of `chromium/src` that gclient checks out at the tag
  pinned in `brave-core/package.json` → `config.projects.chrome.tag`
  (e.g. `156.0.8078.48` on master at research time).
- `package.json` also pins `depot_tools` and declares build scripts. Current
  tooling uses **pnpm** (`devEngines`: node ≥24.16, pnpm ≥11.11); older docs say
  `npm` — same script names: `init`, `sync`, `build`, `apply_patches`,
  `update_patches`, `test`, `start`, `chromium_rebase_l10n`, `create_dist`.

### The checkout (`pnpm run init` / `sync`)

`pnpm run init` (one-time bootstrap; `sync` for updates):

1. Installs JS deps (`pnpm install --frozen-lockfile`), downloads `depot_tools`
   into `vendor/depot_tools`.
2. Writes a `.gclient` manifest with two solutions — `src` (the pinned Chromium
   tag) and `src/brave` (brave-core) — plus `target_os`/`target_cpu`.
3. Runs `gclient sync`: recursively fetches ~**240 repositories**, **~60 GB
   decompressed** (Brave wiki, "Build Deconstructed"). Dep closure comes from
   Chromium's `DEPS` + `brave-core/DEPS` (adds e.g. Sparkle, Omaha, brave
   rust deps) + `EXTRA_DEPS`.
4. Applies every patch in `src/brave/patches/` (tracked via git-ignored
   `*.patchinfo` files), then runs gclient hooks (npm installs in child
   projects, toolchain downloads, `generateAboutCredits`, etc.).
5. Downloads toolchains: Brave's extended Rust toolchain (Chromium's
   `third_party/rust-toolchain` + `wasm32-unknown-unknown` target + `rust-lld`,
   fetched per-platform as an aux package), `third_party/node` binaries, and on
   Windows/macOS the hermetic MSVC/Xcode toolchains
   (`USE_BRAVE_HERMETIC_TOOLCHAIN=1`, `DEPOT_TOOLS_WIN_TOOLCHAIN=1`).

Notable env vars: `GIT_CACHE_PATH` (gclient git cache — essential for repeated
checkouts/CI), `DEPOT_TOOLS_UPDATE=0`, `GSUTIL_ENABLE_LUCI_AUTH=0`,
`PYTHONUNBUFFERED=1`.

`pnpm run sync` re-runs steps 3–5 minimally (compares `.patchinfo` /
`.gclient_entries`; flags: `--init` full reset, `--force` re-pin everything,
`--sync_chromium=false` to skip a Chromium bump, `-D` prune deleted deps).

### Layers of modification, best→worst (Brave's own ordering)

1. **Plain brave-core code** — subclasses/new code living in `src/brave` that
   never touches `src/` directly. For us: `zen/` code sits here.
2. **`chromium_src/` overrides** — a mirror directory; an override file shadows
   the upstream file of the same path during compilation:
   - `#include "chrome/browser/foo.h"` resolves to
     `src/brave/chromium_src/chrome/browser/foo.h` when present.
   - `foo.cc` compiles from the override instead of upstream.
   Idioms: `#define BRAVE_X_H` injected at the end of `public:` in patched
   headers to add members/friends; preprocessor rename tricks to call the
     Chromium impl from the override (`Foo` → `Foo_ChromiumImpl`); whole-file
   stubs to disable features; mojom `[BraveAdd]`/`[BraveExtend]` for
   additive interface changes; TS/JS overrides importing `./foo-chromium.js`
   and re-exporting patched symbols (with `lit_overriding.js` /
   `polymer_overriding.js` helpers for WebUI); Python overrides via
   `brave_chromium_utils.inline_chromium_src_override` + `override_utils`
   decorators (fail loudly when the upstream symbol disappears).
   `tools/chromium_src/check_chromium_src.py` audits stale overrides at rebase.
3. **`rewrite/` plaster** — newer mechanism: per-file `.yaml` regex
   substitutions that *generate* the corresponding `.patch` under `patches/`.
   More robust than hand patches (regexes tolerate upstream drift) and visible
   in-place. Supported repos listed in `patches/.repositories.cfg`.
4. **`patches/` `.patch` files** — last resort, `git apply` diffs against
   `src/`. Naming: path with `-` separators
   (`chrome-browser-ui-views-frame-browser_view_layout.cc.patch`). Regenerate
   with `pnpm run update_patches`; re-apply with `pnpm run apply_patches`.
   Patches that fail on a Chromium bump are fixed by hand
   (`git apply -3`, regenerate). GN/gni patching is funneled through a single
   `import("//brave/build/config/brave_build.gni")` injected by one patch —
   new gni imports go in there, not in new patches.

**Zen overlay rule of thumb** (recommendation, not Brave policy): keep all new
code under `src/brave/zen/` (or `components/zen_*`); touch upstream only via
`chromium_src` and plaster; minimize raw `.patch` count — every `.patch` is a
rebase liability (see §5).

### Build (`pnpm run build`)

- `pnpm run build [Component|Static|Release|Debug] [--target_os --target_arch
  --target]`: applies patches, resolves GN args from `src/brave/.env` + CLI +
  env, runs `gn gen out/<Config>_<platform>`, then `autoninja -C out/<dir>
  brave:all` (default target `brave:all`; `create_dist` for installers).
- Component (default, dev) vs Static vs Release (official). `is_official_build`
  release builds need service keys in `.env` (`brave_services_key`,
  `google_default_client_id/secret`, `brave_stats_api_key`, etc.) — dummies work
  for local; real distribution needs our own values or the codepaths disabled.
- Platform notes: Linux needs ~16GB+ RAM minimum (Gold/LLVM plugin RAM hog on
  release builds), macOS needs Xcode + hermetic toolchain, Windows needs the
  hermetic MSVC download; Android/iOS have their own flows. Cross builds only
  supported from Linux hosts.
- Cadence reality: a cold full Release build is hours-scale even on big machines;
  component builds + `GIT_CACHE_PATH` + a ccache/siso-style remote exec layer
  are how Brave keeps iteration livable. Budget accordingly for CI (§5).

---

## 3. How Brave's ad-blocker is wired

### The engine: `adblock-rust` → `adblock::Engine` via cxx

- `brave/adblock-rust` (crate `adblock`, v0.13, MPL-2.0) implements ABP/uBO filter
  parsing and matching: network rules, cosmetic rules, CSP, `redirect=`/
  scriptlet resource replacements, hosts syntax, uBO syntax extensions,
  iOS content-blocking export, WASM-capable. Feature flags used by Brave:
  `embedded-domain-resolver` is **off** (Chromium's `net::`
  registry-controlled-domain code is used instead — less binary bloat, single
  domain-resolution source of truth); `css-validation`, `content-blocking`,
  `resource-assembler`, `single-thread` (per docs.rs/crate README).
- Vendored into the build as `third_party/rust/adblock/v0_13` (GN `cargo_crate`
  generated by `gnrt` from `third_party/rust/chromium_crates_io` lockfiles) plus
  `adblock-cxx` v1 — a **cxx bridge**, not the old FFI. (The legacy C FFI repo
  `brave/adblock-rust-ffi` is **archived**; it was replaced by cxx-based
  bindings. Anything claiming "link via adblock-rust-ffi" is outdated.)
- C++ side: `#include "brave/components/brave_shields/core/common/adblock/rs/
  src/lib.rs.h"` exposes `adblock::Engine` as `rust::Box<adblock::Engine>`,
  `adblock::FilterSet`, `adblock::BlockerResult`,
  `adblock::BraveCoreResourceStorage`, `adblock::RegexManagerDiscardPolicy`,
  `adblock::FilterRuleInfo` etc.

### Engine + service layer

- `components/brave_shields/content/browser/ad_block_engine.{h,cc}` —
  `AdBlockEngine` wraps one `adblock::Engine` on a checked sequence. API:
  `ShouldStartRequest(url, resource_type, request_initiator, method,
  previously_matched_*) → adblock::BlockerResult`, `GetCspDirectives`,
  `UrlCosmeticResources`, `HiddenClassIdSelectors`, `Load(deserialize,
  DATFileDataBuffer, storage)` / `Load(rust::Box<FilterSet>, storage)`,
  `Serialize()` (compiled-graph cache), `SetupDiscardPolicy` (regex memory
  discarding).
- `ad_block_service.{h,cc}` — `AdBlockService` (keyed service, per
  BrowserContext) owns the *default* engine plus additional engines behind
  `AdBlockEngineWrapper`, and a set of providers:
  - `AdBlockComponentFiltersProvider` (list content shipped via Component
    Updater — the default EasyList-style set + each enabled regional list),
  - `AdBlockFilterListCatalogProvider` (the regional-lists catalog),
  - `AdBlockDefaultResourceProvider` /
    `AdBlockCustomResourceProvider` (scriptlets + redirect resources),
  - `AdBlockSubscriptionFiltersProvider` +
    `AdBlockSubscriptionDownloadManager` (user-subscribed list URLs),
  - `AdBlockCustomFiltersProvider` (user custom rules),
  - `AdBlockLocalhostFiltersProvider`,
  - `AdBlockDATCacheManager` (serialized engine `.dat` in profile — avoids
    reparsing text lists at startup),
  - `AdBlockComponentServiceManager` (component registration),
  - `AdBlockPrefService` (per-site shield prefs).
- `browser/brave_shields/*` wires it to prefs, policies
  (`brave_adblock_policy_handler`), and UI (`brave://adblock`,
  `brave://settings/shields` subpage, `brave://adblock-internals`).

### Request interception path

1. `BraveContentBrowserClient::WillCreateURLLoaderFactory` (in
   `browser/brave_content_browser_client.cc`, subclassing
   `ChromeContentBrowserClient`) calls
   `BraveProxyingURLLoaderFactory::MaybeProxyRequest` — this wraps the
   URLLoaderFactory Chromium was about to hand to the renderer (navigation,
   subresource, worker contexts all covered; WebSockets via
   `WillInterceptWebSocket` + `BraveProxyingWebSocket`; workers covered by
   `chromium_src` overrides on `shared_worker_host` /
   `service_worker_content_settings_proxy_impl`).
2. Each `InProgressRequest` runs `BraveRequestHandlerImpl`'s callback chain
   (`browser/net/`): `OnBeforeURLRequest_SiteHacksWork` →
   **`OnBeforeURLRequest_AdBlockTPPreWork`** → static-redirect → decentralized-
   DNS → header/UA/GPC/service-key helpers → `OnHeadersReceived_*` hooks
   (incl. `brave_ad_block_csp_network_delegate_helper` for `$csp` rules).
3. The adblock helper resolves shields state for the tab (`BraveRequestInfo`
   in `url_context.h`), calls `AdBlockEngine::ShouldStartRequest`, and maps
   `BlockerResult` → block (stub response via `adblock_stub_response`) /
   redirect-to-resource / allow. A follow-up **CNAME-uncloaking** pass resolves
   DNS aliases via `network::HostResolver` and re-queries the engine on the
   canonical name. Devtools instrumentation (`adblock_devtools_instumentation`)
   reports matched rule + source list/line for the Network panel.
4. Request context comes from `browser/net/url_context.cc` +
   `resource_context_data.cc` (per-process context: profile, RFH token,
   resource type, frame ancestry).

### Cosmetic filtering

- `components/cosmetic_filters/` — mojom (`cosmetic_filters.mojom`), a
  renderer-side `CosmeticFiltersJsRenderFrameObserver` + JS handler, and TS
  assets (`resources/data/content_cosmetic.ts`, `procedural_filters.ts`,
  `element_picker.*` for the "block element" picker).
- Flow: at `document_start` the renderer asks the browser for
  `UrlCosmeticResources(url)` → injects a consolidated stylesheet
  (`{display:none !important}` as `cssOrigin: 'user'` — measured ~4% CPU vs
  uBO's ~17% for the same ruleset, per Brave's Cosmetic-Filtering wiki); a
  mutation-observer pass handles late selectors and the first-party heuristics
  in the wiki; `HiddenClassIdSelectors` answers class/id probes per-frame;
  procedural filters (`:has`, `:has-text`, etc.) are evaluated by
  `procedural_filters.ts`.

### Filter lists and delivery

- Shipped as **Component Updater** components, installed/checked at startup and
  every ~5h against `https://go-updater.brave.com/extensions`, payloads from
  `brave-core-ext.s3.brave.com`, packaged by `brave-core-crx-packager`:
  - `iodkpdagapdfkphljnddpjlldadblomo` "Brave Ad Block Updater" — default lists
    (EasyList/EasyPrivacy/uBO-annoyances family + Brave-specific rules),
  - `gkboaolpopklhgplhaaiboijnklogmbc` "Brave Ad Block List Catalog" — enumerates
    regional lists (`brave/adblock-resources` → `filter_lists/list_catalog.json`),
  - `mfddibmblmbccpadfndgakiopmmhebop` "Brave Ad Block Resources Library" —
    scriptlets/redirect resources (from `brave/uBlock` formats via
    `resource-assembler`),
  - one component per enabled regional list (`brave://settings/shields/filters`).
- Custom subscriptions are fetched directly by
  `AdBlockSubscriptionDownloadManager` (no component needed).
- For our fork: point `component_updater` + `brave_domains/service_domains.cc`
  at our own update endpoint, or keep Brave's (see §1 caveat).

### Why a native engine (perf rationale, for the record)

uBO-as-extension was the old Brave path; the Rust engine exists because:
in-process matching on the loader path (no extension IPC per request),
compiled/serialized filter graph (`.dat` cache, ~instant startup),
flatbuffers-backed structures for low memory, regex discard policy for capped
memory, and uBO-compatible `redirect=`/scriptlet semantics without an extension
runtime. The Zen port inherits all of this unchanged under option A.

---

## 4. Where Zen-equivalent UI work lands (scoping only)

- **Vertical tabs sidebar** → extend `browser/ui/views/frame/vertical_tabs/`
  (`VerticalTabStripRegionView`, `VerticalTabStripContainerView`) +
  `browser/ui/tabs/vertical_tab_controller.*`; upstream hooks in
  `chrome/browser/ui/views/frame/` and `chrome/browser/ui/tabs/`. Zen-style
  pinned/essential tabs map to `TabStripModel` groups + pinned tabs;
  tab folders → `tree_tab_node_tab_collection` / `BraveTreeTabStripCollectionDelegate`.
- **Workspaces/spaces** → `browser/ui/views/workspaces/` (bubble + row views,
  save dialog) over Chromium's workspace feature + `TabGroupModel`; persistence
  via `components/sessions`/`SessionService` (Brave already docs tree-tab session
  restore in `docs/chrome/browser/ui/tabs/`). Per-space containers →
  `browser/containers/` + storage partitions.
- **Split view** → `browser/ui/views/frame/split_view/` (`BraveMultiContentsView`)
  over upstream `MultiContentsView`; link-into-split behavior already exists via
  `split_view_link_navigation_throttle`.
- **Glance** (popup preview) → no equivalent; closest primitives are a borderless
  `views::Widget` hosting a `WebContentsView`/`ContentsContainerView` over the
  current contents (same class of thing as `HoverCard`/`TabDialogs`) — a new
  `browser/ui/views/` feature + a pref in `brave://settings`.
- **Compact mode** → layout state in `BrowserView`/`BrowserViewLayout`
  (`chrome/browser/ui/views/frame/browser_view_layout*.cc` — one of the most
  patched files in the tree; flag for rebase risk) plus toolbar/top-chrome
  visibility plumbing (`ExclusiveAccessManager`/`FullscreenController` are the
  analogs for auto-hide chrome).
- **Settings/WebUI pages** → `brave://zen-*` WebUIs next to `browser/ui/webui/`
  and `browser/resources/settings/`; override upstream WebUI via
  `chromium_src` + `lit_overriding.js`/`polymer_overriding.js` per
  `docs/webui_overriding.md`; strings via new `.grd` files + the l10n pipeline.
- **Themeing/Nala** → Brave's Nala design system lives in `ui/` + `resources/`;
  Zen's visual identity rides the same CSS-variable/token layer.
- **Session/profile handling** → Chromium `Profile` keyed services (same as
  `AdBlockService`), `components/prefs` for `zen.*` prefs,
  `components/sessions` + `SessionService` for restore. Migration from Firefox
  profiles is its own workstream (not platform).

---

## 5. Risks

1. **Rebase cadence** — Brave ships a major release ~**every 2 weeks** tracking
   Chromium milestones, plus ~weekly minor/security bumps within ~24h of
   upstream (wiki, Brave-Release-Schedule). Do **not** track `brave-core@master`
   — pin to Brave stable release branches/tags (`1.NN.x`) and advance on our
   own schedule, inheriting Brave's already-resolved Chromium merge. Expect to
   fix our `chromium_src` overrides (`check_chromium_src.py`) and any raw
   patches on each hop; plaster + `chromium_src` carry most of the load.
2. **Patch-conflict surface** — ~1.3k Brave `.patch` files + ~1.6k plasters +
   ~3k `chromium_src` overrides is already a large delta; ours adds to it.
   Rules: no raw `.patch` unless plaster/`chromium_src` can't express it; keep
   Zen code physically separate (`src/brave/zen/`); watch the hot files —
   `browser_view_layout*.cc`, `tab_strip*`, `browser_frame*`, `DEPS`,
   `tools/gritsettings/resource_ids` (ID regeneration — must copy Chromium's
   regenerated file then re-apply, per Brave's own rebase checklist).
3. **Branding/resources** — `brave://*` hosts, `brave_strings.grd`,
   `generated_resources.grd` overrides, product-name string plumbing
   (`chromium_rebase_l10n` rewrites Chromium l10n), icons/product logo, `chrome://
   brave-*` WebUI surfaces (welcome, rewards, wallet, leo, adblock-internals…).
   Rebrand = new `.grd` + host registration + pref/flag defaults; some WebUI
   pages get deleted, some repurposed. Zen uses Crowdin; Brave uses Transifex —
   l10n pipeline must be adapted.
4. **Feature excision** — Rewards/BAT, Wallet, VPN, Leo, News, Talk, Tor
   windows, IPFS, web discovery, P3A telemetry: disable via buildflags/GN args
   where possible; some are deeply wired (Rewards touches shields panel, NTP
   widgets, settings). Decide per-feature: disable-default vs compile-out.
   Keep Shields + adblock intact.
5. **Service endpoints** — filter lists, component updater, variations/Griffin,
   stats, referrals, safe-browsing proxy all point at Brave infra
   (`brave_domains/`, `.env` service keys). Default-off or repoint each
   explicitly; using Brave endpoints without agreement is both a ToS and a
   privacy-story risk.
6. **Build/CI cost** — ~60GB+ source checkout, tens of GB output, hours-scale
   Release builds, per-platform hermetic toolchains, the Brave Rust-toolchain
   aux package (must be rebuilt when Chromium bumps its Clang/Rust revisions —
   Brave has dedicated CI jobs for this; we inherit that operational step).
   CI needs big runners, `GIT_CACHE_PATH` sharing, and remote-exec caching
   (siso/rbe) to be viable.
7. **License obligations** — MPL-2.0 is file-level copyleft: every modified
   MPL file (Brave or Zen) must ship source. Fine for a public fork; just don't
   drop headers/notices, and keep `LICENSE`/attribution intact (incl.
   `chrome://credits` generation).
8. **Update/distribution infra** — Omaha (Win) + Sparkle (macOS) + Linux repos
   are Brave-specific; the port needs its own signing keys, delta-update
   pipeline, and release channels, or a deliberate "no auto-update v1" decision.
9. **Google service keys** — sync, safe browsing, translate, FLoC-adjacent APIs
   need API keys/client IDs (`google_default_client_id/secret`,
   `GOOGLE_API_KEY` for SafeBrowsing per README). Either get our own or disable
   cleanly; shipping with Brave's compiled-in keys is not acceptable.
10. **Android/iOS scope** — brave-core's adblock path is shared, but Zen's
    mobile story is undefined; keep v1 desktop (Linux/macOS/Windows) and cut
    Android/iOS targets from CI early, or it's a multiplier on all the above.

---

## Appendix — key paths in the tree (for the implementers)

- Build/ports: `package.json`, `DEPS`, `build/commands/scripts/{sync,build,commands}.ts`,
  `build/config/brave_build.gni`, `docs/chromium_version_upgrade.md`,
  `docs/patching_and_chromium_src.md`, `docs/plaster.md`,
  `docs/best-practices/{patches,chromium-src-overrides,build-system}.md`.
- Adblock: `components/brave_shields/{content,core}/browser/ad_block_*`,
  `browser/net/brave_ad_block_*`, `browser/net/brave_proxying_url_loader_factory.*`,
  `browser/net/brave_request_handler_impl.*`, `components/cosmetic_filters/`,
  `third_party/rust/adblock/v0_13`, `third_party/rust/adblock_cxx/v1`,
  `browser/ui/webui/brave_adblock*`, `components/brave_adblock_ui/`.
- UI platform to extend: `browser/ui/views/frame/vertical_tabs/`,
  `browser/ui/views/frame/split_view/`, `browser/ui/views/workspaces/`,
  `browser/ui/tabs/`, `browser/containers/`, `browser/ui/webui/`,
  `browser/resources/settings/`.
- Service plumbing: `brave_domains/service_domains.cc`, `.env` (build args),
  `components/brave_component_updater/`, `script/` (tooling).
