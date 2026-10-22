# Chromium Port — Implementation Plan

Synthesized from `FEATURE-INVENTORY.md`, `CHROMIUM-PORT-ARCHITECTURE.md`, and
`PRIOR-ART-AND-ADBLOCK.md` (all in this directory). Read those first; this is
the actionable plan.

## The decision

**Fork `brave/brave-core` into a new repo (recommended name:
`Jackson57279/zen-core`), mount it at `src/brave` inside the Brave-managed
gclient Chromium checkout, and build Zen as a layer on top of Brave.**

Why, in one paragraph: the Brave adblocker is not a library — it is the
`adblock-rust` engine plus request interception, engine lifecycle, cosmetic
filtering, list distribution, and CNAME uncloaking. All of that plumbing is
already written, MPL-2.0 (same license as Zen), and maintained/rebased by
Brave every ~2 weeks. On top of that, brave-core already ships working
implementations of Zen's headline features — vertical tabs, split view,
workspaces, tree tabs (folders), containers — so Zen's UI work extends live
code instead of reimplementing upstream-Chromium stubs. Vanilla Chromium +
adblock-rust alone loses (see architecture doc §1); Electron/CEF/Tauri lose
harder.

**Zen code lives under `src/brave/zen/`** in the new repo. Upstream files are
touched only through `chromium_src/` overrides and `rewrite/` plaster; raw
`.patch` files are the last resort. This repo (`Jackson57279/desktop`) stays
the Firefox/Gecko product and the home of the port specification; the
Chromium-side source lives in the new fork.

**Branch strategy:** pin to Brave stable release branches (`1.NN.x`), never
`master` — Brave rebases onto each Chromium milestone ~every 2 weeks, and we
advance on our own schedule, inheriting their resolved merges.

## What we get for free vs. what we build

| Already in brave-core (extend, don't rebuild) | New work (no equivalent) |
|---|---|
| Vertical tabs (`browser/ui/views/frame/vertical_tabs`, `VerticalTabController`) | Glance (borderless `views::Widget` + `WebContentsView` overlay) |
| Split view (`BraveMultiContentsView`) | Compact mode (layout state in `BrowserViewLayout`) |
| Workspaces (`browser/ui/views/workspaces`) | Spaces model: Zen's workspace semantics, gradients, icons, swipe |
| Tree tabs / folders | Mods (theme packs on the Nala token layer + site CSS injection) |
| Containers (`browser/containers` + storage partitions) | Session model parity (`zen-sessions` over `components/sessions`) |
| Full adblock stack (shields + `adblock-rust`) | Spaces sync backend (Weave has no Chromium analog) |
| Shields UI, element picker, `brave://adblock` | Zen urlbar/global-actions, library panel, media sidebar, kbs |
| | Feature excision: Rewards/Wallet/VPN/Leo/News/Talk/Tor/IPFS |

## Phases

Complexity from the inventory (S ≤ 1w, M 1–3w, L 3–6w, XL > 6w — per a
single engineer, with testing).

### Phase 0 — Infrastructure (M–L)

- Create `Jackson57279/zen-core` as a fork of `brave/brave-core`, branched
  from the latest stable release branch.
- Bootstrap the checkout on a big runner: `pnpm install`, `pnpm run init`
  (~60 GB, ~240 repos), `pnpm run build Component`. Use `GIT_CACHE_PATH`,
  `DEPOT_TOOLS_UPDATE=0`, remote-exec/ccache for iteration.
- Excise Brave features via buildflags/GN args + pref defaults: Rewards/BAT,
  Wallet, VPN, Leo, News, Talk, Tor windows, IPFS, Web Discovery, P3A.
  Keep Shields/adblock intact. Some excision is invasive — expect it to grow.
- Rebrand skeleton: product name + `zen_strings.grd`, `zen://` hosts,
  icons; leave deep string plumbing for Phase 6.
- CI skeleton: one Linux component build; Windows/macOS later.
- **Exit:** a browser that builds, launches with Zen branding, and
  ad-blocks correctly via the inherited shields stack.

### Phase 1 — Sidebar & tab core (L)

Extend `browser/ui/views/frame/vertical_tabs/` +
`browser/ui/tabs/vertical_tab_controller`:

- Zen's sidebar layout: pinned section, essentials (global pinned apps —
  `zen-essential` analog on `TabStripModel`), unpinned section; re-create
  `ZenVerticalTabsManager` layout semantics (single-toolbar mode,
  right-side tabs, window-control placement).
- Pinned semantics: reset-to-URL on click, close-shortcut behavior
  (`ZenPinnedTabManager` → new `ZenPinnedTabController` on `TabStripModel`).
- Zen visual language onto the Nala/token layer (`ui/`, `resources/`):
  squircle radii, `--zen-*` tokens, light/dark schemes.
- Tab renaming, per-tab context menus, essentials promo.

### Phase 2 — Workspaces / Spaces (XL)

- Extend `browser/ui/views/workspaces/` + `TabGroupModel` into Zen spaces:
  named workspaces with icons/gradients (`ZenSpaceManager` analog as a
  keyed service), pinned/unpinned sections, per-space container binding
  (`browser/containers`), swipe switching (`ui/events` gesture tunables —
  replaces `SwipeTracker`/`SimpleGestureEvent` patches).
- Folders → `tree_tab_node_tab_collection` + Zen's `<zen-folder>` UX
  (nesting, collapse, context menus).
- Live folders (RSS/GitHub providers) — own store + provider code.
- Space routing: domain→workspace(+container) rules —
  `NavigationThrottle` on top-level navigations replacing the SessionStore/
  `URILoadingHelper` interception.
- Persistence: per-space tab sets over `components/sessions` /
  `SessionService` (see Phase 5 for full parity).

### Phase 3 — Split view & Glance (L–XL)

- Map `ZenViewSplitter`'s row/column node tree, resize, dropzones onto
  `BraveMultiContentsView`/`BraveContentsContainerView`. Reconcile
  interaction differences (Zen supports up to 4-way splits; check Brave's
  split-view limits first).
- Glance: new `browser/ui/views/` feature — borderless `views::Widget`
  hosting a `WebContentsView`/`ContentsContainerView`, animating from the
  click point, triggered by modifier-click (replace the JSWindowActor pair
  with `WebContentsObserver` + pref). Commit/promote-to-tab behavior.

### Phase 4 — Interaction layer (L)

- Drag & drop: Zen spatial DnD (reorder across spaces, drag-to-split/
  folder/essential) on the tab-strip drag controller; native drop-landing/
  window-move shims per-OS.
- `kbs`: command registry (`cmd_zen*` → Chromium `ui::Accelerator` +
  pref-backed rebind UI).
- Urlbar: floating-URL-bar behavior + new-tab-as-urlbar-session onto
  `OmniboxController`/`AutocompleteProvider`; `ZenUBGlobalActions`
  command-palette results; `ZenSiteDataPanel` → extensions/identity panel
  rework.
- Media sidebar cards (Media Session position/PiP are browser-layer in
  Chromium), downloads arc animation, toasts, emoji picker, sidebar
  notifications, menubar wiring.
- Window drag from content, native share sheet + haptics (`nsIZen*` XPCOM →
  plain C++ services), progress bar.
- Mods: Zen Mods as theme packs — Nala token/style overrides for chrome
  surfaces + per-site CSS injection for content; keep the marketplace web
  surface (install bridge via `WebContents` messaging instead of the
  JSWindowActor pair). **Decision needed:** how far "mods" can go when the
  chrome isn't CSS — scope = Phase-4 spike.
- Welcome/onboarding (essentials picker, default-browser, search engine,
  imports) as `zen://welcome` WebUI.

### Phase 5 — Session & sync (XL)

- `zen-sessions` equivalent: serialize the sidebar object (spaces, tabs,
  folders, splits, essentials, glance/static fields) into profile storage
  over `components/sessions` + `SessionService`; backup/rotation;
  restore-order integration.
- `ZenWindowSync` parity: live cross-window replication of the sidebar
  model via browser-level observers (`TabStripModelObserver` +
  per-window broadcast).
- Spaces sync backend — **decision point**: Weave/FxA has no Chromium
  analog. Options: bespoke service (own server, like the share server), or
  bridge to `components/sync` with a Zen data type. Budget XL either way.

### Phase 6 — Branding, release, distribution (L)

- Full rebrand: `brave://*` → `zen://*` WebUI, `brave_strings.grd`
  replacement, `chromium_rebase_l10n`-style l10n rewrite; adapt Zen's
  ~50-locale Crowdin `.ftl` strings to GRIT/IDS or a WebUI string layer
  (Transifex↔Crowdin pipeline gap — own tooling needed).
- Service endpoints: repoint `brave_domains`/`component_updater` to own
  infra for filter lists/resources/scriptlets — or formalize use of
  Brave's. Own Google API keys (sync/safe-browsing/translate) or disable
  cleanly; never ship Brave's compiled-in keys.
- Update infra: Omaha (Win) + Sparkle (macOS) + Linux repos — or a
  deliberate "no auto-update in v1" decision (record it).
- Signing keys, delta updates, release channels (mirror release/twilight),
  `chrome://credits`/NOTICE obligations (MPL-2.0 file-level copyleft —
  sources stay public, headers intact).
- Adblock settings re-skin: keep `brave://adblock` internals, surface it
  inside Zen's settings pages with Zen language ("privacy protections"
  vs shields naming) — decision on how much Brave shields UI stays.

## Open decisions (need Jackson)

| # | Question | Options | Lean |
|---|----------|---------|------|
| 1 | New repo for the fork? | `Jackson57279/zen-core` (fork of brave/brave-core) recommended | yes |
| 2 | Brave service endpoints (filter lists, component updater) | own infra vs. Brave's (ToS/legal check needed) | own infra long-term, Brave's for bring-up |
| 3 | Spaces sync | bespoke service vs. `components/sync` data type | bespoke (matches Zen share server model) |
| 4 | Mods on non-CSS chrome | Nala token theme packs vs. deeper WebUI theming | spike in Phase 4 |
| 5 | Auto-update v1 | build own Omaha/Sparkle vs. defer | defer to v2 |
| 6 | Mobile | desktop-only v1 | yes |

## Tooling in this directory

- `scripts/bootstrap.sh` — pins + fetches the Brave/Chromium checkout and
  applies the zen overlay.
- `patches/` — quilt-style series for any raw `.patch` that can't be
  expressed as `chromium_src`/plaster (ungoogled-chromium layout:
  `series` file, `core/` vs `extra/`).

## Non-goals for the spec

- Lifting Zen JS/XUL source (impossible — different UI stack; see
  FEATURE-INVENTORY §4 for the full Gecko-dependency table).
- Gecko parity bugs that were fixed in `src/` patches — Chromium doesn't
  have those bugs.
- Android/iOS (cut early; multiplier on everything).
