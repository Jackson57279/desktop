# Prior Art and Adblock Integration — Zen→Chromium Port Research

Research for porting Zen Browser (`Jackson57279/desktop`, fork of `zen-browser/desktop`, Firefox/Gecko-based, MPL-2.0) to a Chromium base with feature parity plus the Brave adblocker (`brave/adblock-rust`).

---

## 1. Prior art

### 1.1 Direct Zen-to-Chromium ports

None found. There is no functional port of Zen onto a Chromium base. The only repo matching "zen chromium" is `Yenkozi/zen-chromium-fork` (0 stars, empty/description-less). Zen's community has discussed the idea (zen-browser/desktop Discussion #15093, Aug 2025) and upstream Zen has no Chromium plans — the port must be a **reimplementation** of Zen's UX on Chromium, not a patch lift: Zen's UI is Gecko JS/XUL (`browser/base/content/zen-*`) + CSS, none of it runs on Chromium's C++/WebUI stack. Feature targets to reimplement: vertical tabs, workspaces, split view, Glance, compact mode, tab folders, Essentials/pinned tabs (per Wikipedia feature list: https://en.wikipedia.org/wiki/Zen_Browser).

### 1.2 Arc-style / sidebar / workspace browsers built on Chromium

| Project | URL | Status (Oct 2026) | Approach | Reusability |
|---|---|---|---|---|
| **Stedding** | https://github.com/ysalitrynskyi/stedding-browser (https://stedding.dev) | Active — committed today; ~14 stars | **Small, documented patch series on stock Chromium 156**; Arc-style sidebar, Spaces, folders, pinned tabs, ⌘T command bar; macOS + Windows; BSD-3-Clause | **Highest**. Exact target feature set, permissive license, clean patch-series methodology that mirrors what we need. Primary reference for UI-layer patching. |
| **The Browser Framework (TBF) + Scout** | https://github.com/Muvels/Scout (TBF repo linked from its README; framework described but not yet published under Muvels/) | Early/active, ~2 stars | Fork of **ungoogled-chromium** + stock Node.js runtime; replaces Chromium's native UI with an **HTML/CSS/TS shell** (`main/`/`shell/`/`shared/` IPC), deliberately Electron-like | Interesting alternative architecture: reimplement Zen's UI in web tech instead of patching native views. Small single-author project, custom "source-available, noncommercial" license — treat as architecture reference only. |
| **Brave** | https://github.com/brave/brave-core | Very active | MPL-2.0 fork of Chromium; **already ships vertical tabs** (`browser/ui/views/tabs/`, `VerticalTabController`, `browser/ui/views/frame/vertical_tabs/`), a right-side sidebar, tab groups, and the adblock engine we want | **Highest for code lifting** — same license as Zen. Vertical-tabs code is real, upstream-maintained C++ we can lift rather than reimplement. Its `chromium_src/` + `patches/` split is also the best-proven model for maintaining a large fork (see §1.3). |
| **Sessionat** | https://github.com/dublyo/sessionat | Active, ~11 stars | Chromium-based Mac browser; workspaces + auto-saved sessions + MCP server; MIT | Minor — workspace/session UX reference only. |
| **Meridian** | https://github.com/Muvels/Meridian | **Archived** (succeeded by Scout) | Electron + React Arc clone | None — demonstrates Electron path is a dead end for full parity (no coherent tab/profile/session/download/extension systems; author said exactly this). |
| **Vivaldi** | https://vivaldi.com | Active | Closed-source; C++ core + JS/HTML UI on Chromium; vertical tabs, workspaces ("tab stacks"), tiling/split view | None for code — but the de-facto reference showing the full Zen feature set is implementable on Chromium. |
| **Sidekick** | https://meetsidekick.com | Active, freemium | Closed-source Chromium fork; sidebar + workspaces | None — closed. Existence proof only. |
| **SigmaOS** | https://sigmaos.com | Active, paid | **WebKit** (not Chromium), SwiftUI, macOS-only | None — wrong engine entirely. |
| **Arc** (The Browser Company) | — | Feature-frozen 2025 | Closed-source Chromium fork; the UX Zen clones | None — closed, but the design spec for Spaces/Essentials/Glance(Peek). |
| **Muon** | https://github.com/brave/muon | Deprecated | Brave's old "HTML shell on Chromium" framework | None — dead. |

### 1.3 How other Chromium forks manage patch series

- **ungoogled-chromium** — the gold standard for patch-series discipline. `patches/` uses the GNU Quilt format: a `patches/series` file lists patches in apply order; patches split into `core/` (must track every Chromium version) and `extra/` (nice-to-have, may rot); every patch must apply with **zero fuzz**, unified diff, `-p1` paths, UTF-8. Config files (`flags.gn`, `substitutions.list`, `domain_substitution.list`, `pruning.list`) keep non-patch data out of diffs. Docs: https://github.com/ungoogled-software/ungoogled-chromium/blob/master/docs/design.md and docs/developing.md. **Recommendation: borrow this layout wholesale for the port's own patches.**
- **Brave (brave-core)** — the model for a *large* fork. Two mechanisms: (a) `patches/*.patch` for small hunks; (b) `chromium_src/<upstream path>` — shadow files that `#include` the upstream source after macro-redefining or subclassing its symbols, so most "patches" survive upstream churn without touching upstream files. Vertical-tabs PR example (https://github.com/brave/brave-core/pull/13816): reviewer forced removal of *all* patch files in favor of `chromium_src` overrides. **This is the structure to imitate: patch files only where overriding is impossible.**
- **Thorium** — cruder: a repo of full replacement files + patches copied into `//chromium/src` by `setup.sh` (`trunk.sh` reverts Chromium to stock first). Easy to reason about, painful to rebase. https://github.com/Alex313031/thorium/wiki/Building, docs/PATCHES.md.
- **Helium** (`imputnet/helium`, GPL-3.0, ~22k stars, active) — modern patch-series fork on the ungoogled-chromium lineage; source archives via `imputnet/chromium-tarballs`, per-platform packaging repos, `deps.ini`-pinned components. Good build-tooling reference; **GPL-3.0 — do not lift code**, read only.
- **Iridium** — stale (tracks old Chromium), skip.

**Net:** no ready-made "Zen on Chromium" exists. Best concrete base: Chromium + ungoogled-chromium-style `patches/series` + brave-core-style `chromium_src` overrides + Brave's vertical-tabs implementation as the seed for the sidebar, with Stedding as the closest open-source UX reference for Spaces/folders/command-bar.

---

## 2. adblock-rust deep-dive

Repo: https://github.com/brave/adblock-rust — crate `adblock` on crates.io, docs at https://docs.rs/adblock, MPL-2.0.

### 2.1 Public API (v0.13, what brave-core vendors today)

- `FilterSet` (https://docs.rs/adblock/latest/adblock/struct.FilterSet.html) — accumulate filter lists before compilation. `FilterSet::new(debug)` then `add_filter_list(list_text, ParseOptions)` per list; `ParseOptions { format, rule_types, permissions }`:
  - `FilterFormat::Standard` — ABP/uBO-style; `FilterFormat::Hosts` — hosts-file lines treated as `||host^`.
  - `RuleTypes::{All, NetworkOnly, CosmeticOnly}` — lets you build separate engines for network vs cosmetic to save memory.
  - `permissions: PermissionMask` — Brave's mechanism to *unlock powerful rule classes only for trusted lists* (e.g. `scriptlet`, `redirect=resource` privileges).
- `Engine` (https://docs.rs/adblock/latest/adblock/struct.Engine.html) — **immutable once built**; rule changes mean building a new engine and swapping.
  - `Engine::new_with_filter_set(set)`, `use_resources(resources)` for `$redirect` bodies and `##+js` scriptlets.
  - `check_network_request(&Request) -> BlockerResult` — block / allow / `redirect` / `csp` / exception decisions.
  - `url_cosmetic_resources(url)` — pre-load cosmetic plan per document (CSS selectors, scriptlets, injected CSS).
  - `hidden_class_id_selectors(classes, ids, exceptions)` — incremental hiding as new DOM classes/ids appear (MutationObserver-driven).
  - Serialization: `serialize`/`deserialize` (flatbuffers `.dat`) — Brave caches compiled engines to disk for fast startup.
- Crate features (README): `css-validation`, `content-blocking` (Apple export), `embedded-domain-resolver` (disable to plug in the host browser's own domain parser — reduces binary bloat, improves consistency), `resource-assembler` (parses uBO's upstream `resources` file formats directly), `single-thread` (fastest matching; disable to get `Send + Sync`).

### 2.2 Filter-format coverage

- EasyList/ABP standard syntax: full network + cosmetic support.
- uBO syntax: broad support — `##+js(...)` scriptlets, `redirect=`/`redirect-rule` with the resource store, `removeparam`, `csp`, `cname` (Brave-added), `generichide`, `$1p`/`$3p`/`$doc`/etc.
- **Known uBO gaps** (open issues, verified in tracker): `:has-text` selector unsupported (brave/adblock-rust#537); nested procedural filters (#495); `html` filters ignored (#4); `$popup` (#683); AdGuard cosmetic variants + AdGuard scriptlet syntax (#236, #324); entity/`*.`-TLD matching partial (#194); wildcard domains in custom filters (#582). Consequence: uBO list-set parity is ~90-95%, not 100% — EasyList-family and Brave's own lists are fully covered; exotic uBO-only features are not.
- Brave's own lists live in **brave/adblock-lists** (MPL-2.0, https://github.com/brave/adblock-lists) — EasyList/EasyPrivacy/unbreak lists plus Brave-maintained regional lists (the `regional` catalog), anti-cookie-consent, anti-social-tracker, debounce lists. Scriptlets/resources in **brave/adblock-resources** (MPL-2.0, https://github.com/brave/adblock-resources). Both consumable directly.

### 2.3 Performance model

- Compile-time heavy, query-time light: rules are compiled into flatbuffer-backed structures; matching is hash/token-driven (no per-rule regex scan). Benchmarks ship in-repo (`benches/bench_matching.rs`, `bench_cosmetic_matching.rs`, `bench_memory.rs`).
- Regexes are lazily compiled behind a `RegexManagerDiscardPolicy` — cold regexes can be discarded under memory pressure.
- Engine is immutable + single-threaded for speed (`single-thread` feature); Brave runs all engine calls on a dedicated sequenced task runner and never on the UI thread (`ad_block_service` uses `AsyncCallAndReplyWithResult` pattern).
- Compiled engine serializes to `.dat` and is mmap-able — avoids re-parsing ~100k+ rules on startup.

### 2.4 Cosmetic filtering / scriptlets

- `Engine::url_cosmetic_resources()` returns per-document instructions: generic CSS selectors, domain-specific selectors, injected CSS, and **scriptlet calls** (`##+js(name, args)` → the `Resource` store supplies the JS body via `resource-assembler`/Brave's adblock-resources).
- Renderer side must: (a) fetch cosmetic plan *before* first paint; (b) inject a base stylesheet + scriptlets into the main world; (c) watch DOM mutations and call `hidden_class_id_selectors` to extend hiding. Brave's implementation: `browser/cosmetic_filters/cosmetic_filters_tab_helper.cc` (browser process) + `components/cosmetic_filters/renderer/cosmetic_filters_js_handler.cc` + `cosmetic_filters_js_render_frame_observer.cc` (Blink-side), resources from `components/cosmetic_filters/browser/cosmetic_filters_resources.cc`.

### 2.5 Regional / subscription lists

- adblock-rust itself has no list-management; Brave layers it on: `components/brave_shields/content/browser/ad_block_service.cc` (default engine), `ad_block_regional_service_manager.*` (a *separate* engine per enabled regional list), `ad_block_subscription_service_manager.*` (user-added custom list URLs), `ad_block_filter_list_catalog_provider.*` + `ad_block_component_installer.*` (catalog download + per-list component updates), `ad_block_dat_cache_manager.*` (compiled-cache persistence). Our port needs an equivalent of this ~browser-layer plumbing — the engine alone is 20% of the feature.

### 2.6 FFI: what Brave actually uses today

- `brave/adblock-rust-ffi` (https://github.com/brave/adblock-rust-ffi) is **archived** — the old hand-written C-ABI staticlib wrapper, retired.
- Brave now uses **Chromium's native Rust toolchain + CXX**: the crate is vendored at `third_party/rust/chromium_crates_io/vendor/adblock-v0_13` with GN targets at `third_party/rust/adblock/v0_13` and a CXX bridge at `third_party/rust/adblock_cxx/v1`. Commit "Replace handwritten adblock rust FFI with CXX based FFI" (brave/brave-core 7d4ebe0, Jul 2023) did the swap. So the modern FFI path = vendored crate + a `cxx::bridge` module + GN `rust_static_library`/`rust_cxx_ffi` targets — **no external FFI repo needed**.

### 2.7 Using adblock-rust without brave-core — Chromium integration sketch

Two viable levels:

**Option A — full Brave-style (recommended):**
1. Vendor `adblock` via Chromium's `//tools/crates` workflow into `third_party/rust/chromium_crates_io` + GN wrapper in `third_party/rust/` (mirroring `adblock` + `adblock_cxx` from brave-core — these are MPL-2.0, liftable nearly verbatim).
2. Write a `cxx::bridge` surface (Engine/FilterSet/Resource/Request/BlockerResult + cosmetics) — adapt `adblock_cxx`'s bridge file.
3. Browser-side `AdBlockService` (BrowserProcessKeyedService) owning the engines on a sequenced task runner.
4. **Request interception hook**: Brave wraps the `URLLoaderFactory` via `browser/net/brave_proxying_url_loader_factory.cc` → `brave_request_handler_impl.cc` → `brave_ad_block_tp_network_delegate_helper.cc` (runs `ShouldBlockRequest` off-thread, also handles `$redirect` to resource bodies and CNAME uncloaking). A lighter-weight alternative is `ContentBrowserClient::CreateURLLoaderThrottles` — a `URLLoaderThrottle` can cancel in `WillStartRequest` and alter the URL in `WillRedirectRequest` (sufficient for block + `$redirect`-to-data-URI; the proxying factory is only strictly needed for body sniffing / full CNAME handling).
5. WebSocket blocking needs a parallel path (Brave: `brave_proxying_web_socket.cc`) — URLLoader throttles do not cover WS.
6. Cosmetic path per §2.4: tab helper + `RenderFrameObserver`/`RenderFrame` observer injecting stylesheet + scriptlets via `gin`/`mojo` — Brave's `components/cosmetic_filters/renderer/` is the direct template.
7. List plumbing per §2.5: catalog + per-list component downloader (can reuse Chromium component updater or simpler periodic fetch), `.dat` cache, per-region engines, custom list subscriptions.

**Option B — minimal C FFI:** revive the archived `adblock-rust-ffi` staticlib approach: `cargo build` the FFI crate, link a C ABI staticlib into a `//components` GN target, call `check_network_request` from a `URLLoaderThrottle`. ~5% of the code but: you inherit an unmaintained FFI layer, hand-marshalled structs, and lose the flatbuffer serialization niceties. Only worth it to dodge Chromium's Rust build integration — which Chromium now ships anyway, so **Option A is strictly better** for a fork that will live with Chromium for years.

---

## 3. Licensing

| Component | License | Notes |
|---|---|---|
| zen-browser/desktop (and the fork) | MPL-2.0 | Verified via GitHub license API. |
| brave-core | MPL-2.0 | Verified. All `chromium_src`, `browser/`, `components/` code liftable. |
| brave/adblock-rust | MPL-2.0 | Verified (README badge + license API). |
| brave/adblock-lists, brave/adblock-resources | MPL-2.0 | Verified — lists and scriptlets are legally consumable. |
| ungoogled-chromium | BSD-3-Clause | Liftable. |
| Stedding | BSD-3-Clause | Liftable (attribution line in notices). |
| **Helium** | **GPL-3.0** | Reference only — GPL patches can't enter an MPL-2.0 codebase without relicensing the derivative; Chromium itself is BSD so a GPL-3.0 *browser* is possible, but keep our fork clean: read, don't copy. |
| Chromium | BSD-3-Clause (multi) | Fork must keep upstream `LICENSE`/`NOTICE` attribution. |
| Scout/TBF | Custom "source-available, noncommercial" | Architecture reference only — not liftable for an MPL/commercial-friendly project. |

**MPL-2.0 mechanics:** file-level copyleft — lifted Brave files stay MPL-2.0 (fine, same license as Zen); must retain license headers + copyright notices; modifications to covered files must remain available, new files can be anything. Exhibit-B doesn't apply (Brave doesn't use it). Practically: lifting brave-core code into the port is a license-neutral operation; just preserve headers and ship a `NOTICE` attributing Brave Software + Zen contributors.

## 4. Security / stability considerations

- **Native (pre-network) vs extension blocking**: in-stack filtering via proxying factory/throttle sees every subresource incl. WS, applies `$redirect`, does CNAME uncloaking, and is immune to MV3 limits (MV3 caps static DNR rulesets, kills dynamic scriptlet injection and response substitution). Extension-based blocking (Helium's bundled-component-extension approach) is the fallback: simpler, but inherits MV3 gaps. For "Brave adblocker" parity, in-stack is required anyway.
- **Engine is untrusted-input-adjacent**: filter lists are remote, community-authored. Adopt Brave's `PermissionMask` model — strip `scriptlet`/`redirect=resource` permissions from untrusted custom lists (malicious lists could otherwise run JS on any page).
- **Threading/hot path**: never call the engine on UI/IO threads; single-thread-mode engine + sequenced task runner + immutable swap-on-update (Brave's pattern). A rebuilt engine must fully replace, never mutate live.
- **CNAME uncloaking**: requires DNS resolution before the filter check (Brave does it inside `brave_ad_block_tp_network_delegate_helper` gated by `kBraveAdblockCnameUncloaking`, disabled under Tor/proxy contexts). If we skip CNAME uncloaking in v1, keep the design room for it.
- **`$redirect` bodies**: served from the bundled resource store — validate MIME + never sniff; scriptlet resources are JS injected into the main world → treat the resource store as part of the trusted compute base (verify at build/download time, ship via component updates not arbitrary web fetches).
- **Fingerprinting surface**: adblocking alone ≠ fingerprinting protection. Brave's anti-fingerprinting is a *separate* feature ("farbling" in `components/brave_shields`) that perturbs WebGL/audio/canvas/font APIs — it breaks sites and is out of scope; note that shipping Shields-style network blocking without farbling still changes the fingerprint (blocks change page behavior vs vanilla Chrome — this is normal and acceptable; document rather than chase parity).
- **Stability risks of the fork itself**: UI patches on `//chrome/browser/ui` are the highest-churn upstream area — vertical-tab patches will need per-release attention (Brave absorbs this continuously). Keep the patch surface minimal via `chromium_src` overrides; pin engine `.dat` format version to the crate version to avoid stale-cache crashes.
- **List-update channel**: a list is remote-executed policy — serve over HTTPS, pin by hash where possible, rate-limit updates, and treat the regional catalog as first-party data (Brave publishes theirs; reuse `brave/adblock-lists` initially).

## 5. Recommendation

- **Base**: stock Chromium + `patches/series` (ungoogled-chromium discipline) + `chromium_src/` overrides (brave-core discipline). Seed the sidebar/workspaces UI from **Brave's `browser/ui/views/tabs` vertical-tabs code** (MPL-2.0, maintained, closest feature) and consult **Stedding** for Spaces/folders/command-bar UX patterns. Reimplement Zen-only features (Glance, split view, workspaces, compact mode) natively — no shortcuts exist.
- **Adblock**: vendor the `adblock` crate via `//tools/crates`, adapt brave-core's `third_party/rust/adblock_cxx` bridge + `components/brave_shields/content/browser/ad_block_service.*` + `components/cosmetic_filters/` + `browser/net/brave_proxying_url_loader_factory.cc` pipeline. Consume `brave/adblock-lists` + `brave/adblock-resources` for defaults and regional lists. Budget: the engine binding is days; the list/plumbing/cosmetic/renderer plumbing is the real work (weeks).
