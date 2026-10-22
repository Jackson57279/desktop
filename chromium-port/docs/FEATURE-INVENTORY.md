# Zen Browser → Chromium Port: Feature Inventory

Scope: full enumeration of `src/zen/` modules plus the non-Zen patch overlay
(`src/browser`, `src/toolkit`, `src/layout`, `src/dom`, `src/widget`, `src/gfx`,
etc.) with a categorization for porting to a Chromium base.

Categories used below:

- **(a) UX** — front-end UX re-implementable in a Chromium UI layer (side panel /
  WebUI / views).
- **(b) Engine** — needs C++ work inside Chromium (blink / cc / browser /
  renderer).
- **(c) Build/Branding** — build, packaging, branding, prefs config.
- **(d) Gecko-only** — Firefox-specific with no Chromium analog; needs a design
  replacement, not a port.

LOC figures count code-bearing files (`.mjs/.js/.ts/.css/.cpp/.h/.xhtml/.json`)
under each `src/zen/` directory.

---

## 1. `src/zen/` feature inventory

### spaces/ — Workspaces (9,110 LOC, 15 files)

| File | Role |
|---|---|
| `ZenSpaceManager.mjs` (~3,700) | Core workspace model + UI controller: workspace list, pinned/unpinned sections, switch animations, workspace context menus, bookmark menus integration, swipe/scroll workspace switching |
| `ZenSpace.mjs` | `<zen-workspace>` custom XUL element (sidebar workspace container), `nsZenCollapsiblePins` (collapsible pinned section reusing `nsZenFolder`) |
| `ZenSpaceCreation.mjs`, `ZenSpaceIcons.mjs` | Workspace creation form UI, icon/emoji picker for workspaces |
| `ZenGradientGenerator.mjs` | Per-workspace gradient/theme picker (accent color → CSS gradient) |
| `ZenSpacesSwipe.mjs`, `ZenSpaceAddSwipe.mjs`, `nsZenSwipeOverrides.cpp` | Touchpad/trackpad swipe to switch workspaces; C++ file overrides `SwipeTracker` consumer thresholds (paired with `dom/webidl/SimpleGestureEvent` patch) |
| `ZenSpaceBookmarksStorage.js` | Places-backed persistence: `zen_bookmarks_workspaces` + `_changes` SQLite tables in Places DB |

**Hooks:** tabbrowser DOM (tab elements carry `zen-workspace-id`), `gBrowser`,
Places DB, `Services.prefs`, Fluent l10n, native `SwipeTracker`/`SimpleGestureEvent`.
**Category:** (a) front-end core + (b) engine bits for swipe physics + (d)
Places-table storage. **Complexity: XL** — the flagship feature.

### split-view/ — Split Screen (3,120 LOC, 5 files)

`ZenViewSplitter.mjs` (~2,660 LOC) lays multiple tabs out as a node tree
(row/column split nodes) inside `tabbrowser-tabpanels`, with resize handles,
dropzones, rearrangement, split-group indicators. Hooks: `<tabbrowser>` /
`tabbrowser-tabpanels` DOM, tab attributes (`split-view-group`).
**Category:** (a) — needs Chromium multi-WebContents compositing in one tab.
**Complexity: XL** (requires an actual "two WebContents side-by-side" layout,
which Chromium's tab model doesn't natively expose to UI).

### glance/ — Link-Preview Overlay (2,604 LOC, 8 files)

`ZenGlanceManager.mjs` (~2,100) opens a link preview in a **remote XUL
`<browser type="content">`** overlay animating from the click point;
expand-to-tab, detached glance, close-on-outside-click.
`actors/ZenGlance{Parent,Child}.sys.mjs` — JSWindowActor pair intercepting
modifier+click (ctrl/alt/shift/meta), sending URL + triggering principal to
chrome. `zen-glance.inc.xhtml` overlay markup.
Hooks: XUL `browser` elements, `maychangeremoteness`, `userContextId`,
JSWindowActor IPC, `MozXULElement.parseXULToFragment`.
**Category:** (a) + IPC via WebContents->browser-side messaging. **Complexity: L.**

### compact-mode/ — Auto-Hide Chrome (1,962 LOC, 15 files)

`ZenCompactMode.mjs` toggles/collapses sidebar+toolbar, hover-reveal with
hysteresis, fullscreen interactions. Native **C++ XPCOM `ZenMouseTracker`**
(`.cpp` + `ZenMouseTrackerWin.cpp` + `ZenMouseTrackerCocoa.mm`) tracks the
pointer *outside* the window edges and posts `zen-mouse-tracker:exited`
observer notifications. `nsIZenMouseTracker.idl` contract.
Hooks: XPCOM service, DOM prefs (`zen.view.compact.*`), window/document
attributes driving CSS.
**Category:** (a) mostly UI + small (b) platform mouse tracking.
**Complexity: M.**

### mods/ — Zen Mods (1,253 LOC, 11 files)

User-installable **CSS themes applied to the browser chrome document**:
`ZenMods.mjs` downloads/updates/enables mods, rebuilds `zen-themes.css` in the
profile `chrome/` dir; `nsZenModsBackend` + `ZenStyleSheetCache.cpp` inject the
generated stylesheet into the chrome docshell (`dom/base/Document.cpp` patch);
`ZenModsMarketplace` JSWindowActor pair bridges the mods website
(zen-browser.app store) to install/uninstall buttons.
Hooks: profile FS, stylesheet service, JSWindowActor, chrome-document CSS.
**Category:** (d) — this is "userChrome.css as a product". On Chromium the
chrome UI isn't CSS-driven; needs a theming system for the new UI layer, plus
the marketplace can be kept as web content. **Complexity: L.**

### folders/ + live-folders/ — Tab Folders (5,165 LOC)

- `folders/ZenFolders.mjs` (~2,400), `ZenFolder.mjs` — `<zen-folder>` custom
  element built on **Firefox tab-group machinery** (patched `tabgroup.mjs`),
  nested subfolders (pref-capped), collapse/expand animations, context menus.
- `live-folders/` — auto-populated folders from providers (`RssLiveFolder`,
  `GithubLiveFolder`), persisted to `zen-live-folders.jsonlz4`, synced via
  `ZenWindowSync`, `ZenLiveFoldersUI` in the sidebar.

Hooks: tab-group elements, `TabStateCache`, Places-ish JSON storage,
observer/category startup.
**Category:** (a). **Complexity: M** (Chromium has tab groups; the Zen
interaction model is bespoke UI work).

### kbs/ — Keyboard Shortcuts (1,672 LOC, 2 files)

`ZenKeyboardShortcuts.mjs` — full customizable hotkey system: VK→key maps,
command registry (`cmd_zen*`), shortcuts UI, per-command rebinding, defaults
shipped via `prefs/zen/kbs.yaml`. Hooks: XUL `<keyset>`/`command` DOM, prefs.
**Category:** (a). **Complexity: M.**

### media/ — Sidebar Media Controls (1,310 LOC, 6 files)

`ZenMediaController.sys.mjs` — stacked media cards in the sidebar (title,
artist, progress, seek, PiP, focus), driven by **`MediaController` WebIDL
extensions Zen patched into Gecko** (`dom/chrome-webidl/MediaController.webidl`:
`getPositionState()`, `isBeingUsedInPIPModeOrFullscreen`) plus
`toolkit/components/pictureinpicture` integration. `zen-media-controls.inc.xhtml`
overlay.
Hooks: `MediaController`/`MediaStatusManager` C++ (patched in `dom/media/`),
PiP module, UIComponent framework.
**Category:** (b) small engine API + (a) cards UI — Chromium has Media Session
APIs (position state, PiP) but they live in browser/service layer.
**Complexity: M.**

### downloads/ — Download Arc Animation (627 LOC, 4 files)

`ZenDownloadAnimation.mjs` — arc flight animation from last-click point to the
downloads button on download start; listens to `Downloads.getList()`.
Hooks: `Downloads` JSM API, `gZenUIManager._lastClickPosition`.
**Category:** (a). **Complexity: S.**

### share/ — Share Spaces/Folders (1,538 LOC, 8 files)

`ZenShareManager.mjs` serializes spaces/folders/split-views →
`ZenShareClient.sys.mjs` uploads to a **Zen share server** (schema in
`share.schema.json`), copies link; import path recreates them locally.
`zen-share-confirm.inc.xhtml` dialog. Companion C++ `nsIZenCommonUtils.share`
(native OS share sheet, `toolkit/common/`).
Hooks: fetch to Zen service, clipboard, tab serialization, native share.
**Category:** (a) + service endpoint (portable) + small native share (b).
**Complexity: M.**

### sync/ — Firefox Sync for Spaces (2,028 LOC, 4 files)

`ZenSpacesSync.sys.mjs` registers a **custom Weave/Sync engine**
(`SyncEngine`/`Store`/`Tracker`/`CryptoWrapper` subclasses) — the `spaces`
collection syncs workspace projections to Firefox Accounts; `ZenSpacesSyncModel`
diffs projections, `ZenSpacesSyncApplier` reconciles incoming batches;
`services/sync/modules/service-sys-mjs.patch` registers the engine in `Service`.
Hooks: entire `resource://services-sync/*` stack, FxA, ContextualIdentityService
topics.
**Category:** (d) — Firefox Sync has no Chromium analog. Needs a bespoke sync
backend (own server, or adapt Chromium's sync). **Complexity: XL.**

### sessionstore/ — Zen Sessions + Window Sync (2,852 LOC, 4 files)

- `ZenSessionManager.sys.mjs` (~1,080): Zen's own session store —
  `zen-sessions.jsonlz4` capturing **sidebar object** (workspaces, tabs, groups,
  folders, split-view state), backups/rotation, restore integration into
  Firefox `SessionStore`/`SessionStartup`/`TabStateFlusher`.
- `ZenWindowSync.sys.mjs` (~1,770): **live cross-window tab sync** — every tab
  event (`TabOpen/Close/Move/Pinned/Essential/Split/Group...`) broadcasts via
  `TabStateCache`+`TabStateFlusher` to all windows, with `zen-unsynced-window`
  opt-out and `zen-empty-tab` placeholders. Hooks `SessionStore` observers.

**Category:** (b) session model + (d) — window-sync semantics need a
Chromium-side replication layer over `sessions`/tab model. **Complexity: XL.**

### space-routing/ — Domain → Workspace Routing (1,744 LOC, 9 files)

`ZenSpaceRoutingManager.sys.mjs` — persisted rules (`zen-space-routing.jsonlz4`)
mapping domains to workspaces + container (`userContextId`);
`ZenSpaceRoutingNavigation` intercepts **every top-level navigation** and
re-opens routed URLs in the target space/container;
`ZenSpaceRoutingDialog` + context-menu entry to add routes.
Hooks: navigation pipeline (tab-context menu, `addTab` interception, contextual
identity), JSONFile storage.
**Category:** (a) + (b) navigation interception. **Complexity: L.**

### urlbar/ — URL Bar (2,795 LOC, 8 files)

- `ZenUrlbarController.mjs` — floating-URL-bar behavior, **new-tab =
  urlbar-session** replacement (closing the session returns you to your tab),
  domain-only sidebar display.
- `ZenUBProvider.sys.mjs` + `ZenUBActionsProvider` + `ZenUBGlobalActions`
  (~1,100) — command-palette style global actions ("toggle compact mode",
  "open library") inside the urlbar.
- `ZenUBSidebarProvider` — urlbar results rendered in the sidebar.
- `ZenUBResultsLearner` — custom ranking/learning for Zen results.
- `ZenSiteDataPanel.sys.mjs` (~1,026) — replaces the identity/permissions
  panel: unified extensions panel rebuild, boosts button, site data icon.

Hooks: `UrlbarProvider` framework, `UrlbarInput`, `gUnifiedExtensions`, XUL
popups. Paired with heavy `browser/components/urlbar/*` patches
(`UrlbarInputBase-mjs` +395 LOC, UrlbarView, UrlbarShared).
**Category:** (a) + (d) Urlbar provider framework → Chromium omnibox
(`AutocompleteProvider`/`OmniboxController`). **Complexity: L.**

### library/ — Zen Library (8,392 LOC, 18 files)

`ZenLibrary.mjs` (MozLitElement, ~1,066) — unified panel replacing Firefox's
Library window: sections `ZenLibraryHistorySection`, `ZenLibraryDownloadsSection`,
`ZenLibraryBoostsSection`, `ZenLibraryMediaSection`, `ZenLibrarySpacesSection`,
`ZenLibrarySearchSection`; `ZenLibraryWidget` registers a CustomizableUI
toolbar widget with download-progress badge; `ZenLibraryMediaPreview` hover
previews; `ZenLibraryDragAndDrop`; native `nsZenLibraryFiles` C++ scans disk
folders for media files (audio/video) for the media section.
Hooks: `DownloadsCommon`/`DownloadsViewUI`, Places history queries,
CustomizableUI, Lit (vendored by Mozilla), native FS scanner XPCOM.
**Category:** (a) UI + small (b) file-scanning. **Complexity: L.**

### boosts/ — Per-Site Appearance + Zap (7,921 LOC, 25 files)

Two sub-features:
1. **Boosts**: per-domain color transforms — invert/darken pages or tint with a
   gradient accent. `ZenBoostsManager.sys.mjs` persists boosts
   (`zen-boosts.jsonlz4`); `ZenBoostStyles`/`ZenBoostsEditor` UI;
   `ZenSelectorComponent` picker UI. Engine side is the **deepest Gecko
   integration in the repo**: `BrowsingContext` carries `zenBoostsData`,
   `zenBoostsComplementaryRotation`, `isZenBoostsInverted` fields
   (`docshell/base/BrowsingContext.h` + `dom/chrome-webidl/BrowsingContext.webidl`
   patches); `nsZenBoostsBackend.cpp` implements `ResolveStyleColor()` hooked
   into `StyleColor::CalcColor/ResolveColor` (`layout/style/StyleColor.cpp`),
   fanned out through ~25 layout patches (gradients, SVG context paint, text
   frames, tables, scrollbars, compositor animations, canvas, `nsAttrValue`,
   `nsPresContext` SVG-image override).
2. **Zap**: `ZenZapBar`/`ZenZapDissolve` — per-site element remover ("Zap this")
   with a **WebGL particle dissolve** effect (`ZenZapOverlayChild`, boosts
   actors JSWindowActor pair).

Hooks: Servo style system, `BrowsingContext`, compositor, WebGL, JSWindowActor.
**Category:** (b) — equivalent requires Blink/cc color-resolution hooks or a
render-process color filter. The Zap UX is (a). **Complexity: XL.**

### tabs/ — Vertical Tabs, Essentials, Pinned (3,050 LOC, 10 files)

- `ZenPinnedTabManager.mjs` (~1,190) — pinned-tab semantics: reset-to-URL on
  click, close-shortcut behavior, **essentials** (always-visible global pinned
  apps, `zen-essential` attribute, max via pref).
- `ZenTabRenamer.mjs` — inline renaming of tabs/workspaces/folders.
- `ZenEssentialsPromo.mjs` — promo element. CSS: `zen-tabs.css`,
  `vertical-tabs*.inc.css` — the whole vertical-tabs skin.

Hooks: tab DOM (`tab.pinned`, `zen-essential`, `visuallyselected` attribute
patch), `E10SUtils`, tab context menus.
**Category:** (a). **Complexity: L.**

### drag-and-drop/ — Tab Strip DnD (2,412 LOC, 9 files)

`ZenDragAndDrop.js` (~2,200) — **class replacing Firefox's `TabDragAndDrop`**
(patched in `browser/components/tabbrowser/content/drag-and-drop-js.patch`):
spatial dragging of tabs/groups/folders across workspaces, drop targets,
multi-tab moves, drag-to-create splits/folders/essentials.
Native `nsZenDragAndDrop` XPCOM: drag-image opacity, drop-landing detection,
`nsZenNativeWindowMove.mm` (macOS native window move during drag),
`nsZenDropLandingCocoa.mm`.
Hooks: tabbrowser DnD pipeline, native drag services.
**Category:** (a) + small (b) native. **Complexity: L.**

### common/ — Shared Infra + Theme System (8,785 LOC, 48 files)

| File | Role |
|---|---|
| `ZenUIManager.mjs` | Window glue: popup tracking, last-click position, tabs-toolbar updates, compact-mode button, toast/urlbar init |
| `ZenVerticalTabsManager.mjs` | **Layout engine for the chrome**: arranges sidebar/nav-bar/toolbox per prefs, single-toolbar mode, right-side tabs, window-control placement |
| `ZenStartup.mjs` | Ordered per-window startup (layout init → workspaces → UI), watermark |
| `ZenCustomizableUI.sys.mjs` | Registers `zen-sidebar-top-buttons`/`zen-sidebar-foot-buttons` toolbar areas + library widget in CustomizableUI |
| `ZenActorsManager.sys.mjs` | Registers all JSWindowActors (mods marketplace, glance, window drag, boosts) |
| `ZenMenubar.mjs`, `ZenToasts.mjs`, `ZenSidebarNotification.mjs`, `ZenEmojiPicker.mjs` | App menu, toasts, sidebar notification UI, emoji picker (workspace icons) |
| `ZenUpdates.mjs` | Post-update full-window sweep animation |
| `ZenSessionStore.mjs` | Restores zen tab data from Firefox tab state |
| `zenThemeModifier.js` | Injects `--zen-primary-color`, `--zen-border-radius`, `--zen-element-separation` into chrome docs |
| `zen-sets.js` | `cmd_zen*` command dispatch table |
| `ZenUIMigration.sys.mjs`, `ZenPGOTraining.sys.mjs`, `ZenProgressBar.sys.mjs`, `ZenUIComponent.sys.mjs` | Settings migrations, PGO training tour (build perf), progress bar, base class |
| `common/styles/*` | ~2,500 LOC CSS: squircle, themes (light/dark schemes), panels, toolbar, popup, backdrop-filter SVG, branding |

Hooks: `gNavToolbox`, `gBrowser`, CustomizableUI internals, prefs, Fluent.
**Category:** (a) for all of it; the theme system is (d) insofar as Chromium
UI isn't CSS — but if the new UI layer is web-based it ports directly.
**Complexity: L.**

### welcome/ — Onboarding (1,355 LOC, 4 files)

`ZenWelcome.mjs` — first-run flow: pick essentials, choose default browser,
search engine, optional uBlock install via `AddonManager`/`AddonRepository`,
imports. `welcome-background.mp4` asset.
Hooks: `AddonManager` (WebExtensions!), `SearchService`, `CustomizableUI`,
ShellService.
**Category:** (a) + (d) extension install → chrome Web Store flow.
**Complexity: M.**

### sessionstore/ — see above.

### toolkit/ — Native Share + Haptics (262 LOC, 12 files)

`nsIZenCommonUtils.idl` XPCOM: `share()` via native share sheet
(`ZenShareInternal.mm` macOS, `ZenShareInternal.cpp` Windows) +
`playHapticFeedback()` (`ZenHapticFeedback.mm` macOS).
**Category:** (b) small native shims. **Complexity: S.**

### window-drag/ — Drag Window from Content (481 LOC, 7 files)

`ZenWindowDrag` JSWindowActor pair: mousedown on empty page areas → native
window move via `nsZenWindowDragUtils` XPCOM.
**Category:** (b) — needs WebContents input routing → `views::Widget` move.
**Complexity: M.**

### vendor/ — Vendored Libs (4 LOC, 3 files)

`motion.min.mjs` — vendored Motion v13 animation lib (+ `motion.dep` pointer).
**Category:** (c) — re-vendor or npm dep. **Complexity: S.**

### fonts/, images/ — Static Assets

Fonts (3 files), UI images/layout SVGs/favicons (18 files). **(c). S.**

### @types/ — TypeScript Definitions (79,746 LOC, 13 files)

Generated `.d.ts` for Gecko internals (xpcom, xpild, dom, glean, win32/darwin/
linux). Dev-tooling only; regenerate for the new codebase. **(d). S.**

### tests/ — Test Suite (115,941 LOC, 962 files)

Zen feature tests (`tests/{spaces,glance,folders,split_view,compact_mode,media,
library,live-folders,share,space_routing,urlbar,ub-actions,window_drag,
window_sync,device_sync,...}`) plus **vendored upstream mochitests**
(`tests/mochitests/*`) covering patched Firefox internals (sessionstore,
safebrowsing, PiP, gestures, plugins, shells) and gtests for boosts/windows
shell. `ignorePrefs.json` + `manifest.toml`.
**Category:** (c)/(d) — test harness is mochitest; feature tests port to
whatever harness the Chromium build uses. **Complexity: L** (mostly rewrite).

### Cross-cutting zen/ files

- `ZenComponents.manifest` — Gecko **category registrations**
  (`browser-before-ui-startup`, `browser-window-delayed-startup`,
  `browser-quit-application-granted`) wiring every Zen service.
- `moz.build` / `jar.inc.mn` per dir — jar packaging into `browser` chrome.
- `zen.globals.mjs` — ESLint globals list (all `gZen*` singletons).

---

## 2. Non-zen patches in `src/` (Firefox internals surgery)

~250 `.patch` files. UI-relevant groupings:

### `src/browser/` — 115 patches (the biggest surface)

- **Layout surgery** (`base/content/browser.xhtml`, `browser-box.inc.xhtml`,
  `navigator-toolbox.inc.xhtml`): wraps the app in `zen-main-app-wrapper`,
  inserts `zen-essentials`, `zen-tabs-wrapper`, `zen-appcontent-wrapper`,
  `zen-tabbox-wrapper`, `zen-library`, media-controls include; re-parents
  `nav-bar`/`PersonalToolbar`. → **(a)/(b)**: a Chromium port needs a custom
  top-level UI layout anyway; no direct analog.
- **Tabbrowser** (`Tabbrowser.sys-mjs` +434, `tab-js`, `tabs-mjs`,
  `tabgroup-mjs`, `drag-and-drop-js`, `tab-context-menu-js`,
  `TabsList-sys-mjs`, `AsyncTabSwitcher`, `TabUnloader`, `ctrlTab`):
  adds `zen-workspace-id`, `zen-essential`, `zen-empty-tab`, `zen-glance-tab`,
  `split-view-group` tab semantics; folders implemented as special tab groups;
  Zen DnD replaces upstream; context-menu items; `ZenWorkspaces` calls wired
  through `Tabbrowser`. → **(a)/(d)**: Chromium has no tab DOM; needs a custom
  tab-strip model + `TabModel`/`TabStripModel` extensions.
- **SessionStore** (`SessionStore`, `SessionSaver`, `SessionFile`,
  `SessionStartup`, `TabState`, `TabStateFlusher`, `TabGroupState`):
  persists `zenWorkspace`, `zenEssential`, `zenSyncId`, glance/static fields
  per-tab; `ZenWorkspaceDataChanged` observer topic; `isZenUnsynced` windows;
  empty-tab filtering. → **(b)/(d)**.
- **CustomizableUI** (`CustomizableUI-sys-mjs` +77, `CustomizeMode`,
  `panelUI`, `ToolbarContextMenu`): registers zen sidebar areas, library
  widget, extension overflow into urlbar. → **(d)**.
- **Urlbar** (`UrlbarInputBase-mjs` +395, `UrlbarView`, `UrlbarShared`,
  `UrlbarPrefs`, `UrlbarProvidersManager`, `UrlbarProviderHeuristicFallback`,
  `UrlbarProviderTopSites`, `UrlbarMuxerStandard`, `UrlbarValueFormatter`,
  `UrlbarChildController`): floating urlbar, zen providers, view layout. → (d)
- **Places/Bookmarks** (`editBookmark-js` +134, `editBookmarkPanel-inc-xhtml`,
  `PlacesUIUtils-sys-mjs`, `browserPlacesViews-js`, `bookmarkProperties`):
  workspace-aware bookmarks UI. → (a)/(d) Places → Chromium bookmark model.
- **Preferences** (`preferences-xhtml`, `main-inc-xhtml`,
  `zenLooksAndFeel.inc.xhtml`, `main-js`, `account-sync-mjs` (adds "spaces"
  to sync list), `aiFeatures-mjs` (disables AI features), `syncChooseWhatToSync`):
  Zen settings pages + FxA integration. → (a)/(c).
- **Extensions** (`ext-browser-js`, `ext-tabs-js`): API tweaks for zen tab
  attrs. **Toolkit** `ext-runtime`, `ext-tabs-base`. → (d) Chromium
  `extensions/` API differs.
- **Sidebar** (`browser-sidebar-js`): Firefox sidebar disabled/adapted. → (d).
- **Misc UI**: `browser-init/browser-js/browser-sets/browser-commands`
  (zen command wiring), `aboutDialog` (Zen branding), `aboutwelcome`,
  `aboutlogins`, `identityPanel`, `nsContextMenu`, `pageActions`,
  `fullScreenAndPointerLock`, `gestureSupport`, `main-popupset`,
  `BrowserGlue/BrowserContentHandler/BrowserDOMWindow/URILoadingHelper/
  BrowserWindowTracker` (space routing + window tracking), `screenshots`
  overlay, `SearchOneOffs/SearchUIUtils`, `AIWindow`, `FeatureCallout`,
  `ContainerCreationPanel` (container UX), `newtab ActivityStream`,
  `EncryptedMediaParent/WebRTCParent`.
- **Themes** (`browser-shared-css`, `tabs-css`, `urlbar-css`,
  `identity-block-css`, `toolbarbuttons-css`, `content-area-css`,
  `ctrlTab-css`, per-OS `browser-css`, `places/organizer-css`, `aboutReader`,
  `in-content`, design-system tokens, `tree`, `menulist`, PiP player).
- **Packaging**: `jar-mn`, `package-manifest-in`, NSIS installer/branding,
  `installer/custom.properties`.

### `src/toolkit/` — 38 patches

Widgets (`tabbox`, `arrowscrollbox`, `browser-custom-element`, `findbar`,
`panel`, `xul-css`, `moz-toggle` tokens, `tree`, `menulist`, `infobar`),
`PopupNotifications`, `ShortcutUtils`, `LightweightThemeConsumer`,
`UpdateUtils` (update server), `AddonManager`, `Blocklist`,
`AddonUpdateChecker`, ext-runtime/ext-tabs-base, `JSONFile`, `AppConstants`,
`nsToolkitProfileService`, `nsXREDirProvider` (profile dirs), updater/makensis,
themes. → mostly **(d)** XUL-widget + (b)/(c) profile/update plumbing.

### `src/layout/` + `src/dom/` + `src/gfx/` + `src/servo/` — boosts + gestures

- The **boosts color-filter pipeline**: `StyleColor::CalcColor/ResolveColor` →
  `nsZenBoostsBackend::ResolveStyleColor`, threaded through `nsCSSRendering*`
  (gradients, borders, image renderer), `SVG*` (context paint, gradients, text,
  image context), `nsTextFrame`/`nsTextPaintStyle`, `nsTableFrame`,
  `nsTreeBodyFrame`, `ScrollbarDrawing`, `gfxUtils` (`ToDeviceColor(frame)`),
  `gfx/layers` animation storage, `ComputedStyle`, `StyleAnimationValue`,
  `ServoCSSParser`, `nsStyleStruct`, `nsAttrValue`, `Document` (mods sheet),
  `CanvasRenderingContext2D`, `PresShell`/`nsPresContext` (SVG override). → **(b) XL.**
- `BrowsingContext` fields (`docshell/BrowsingContext.h` +
  `dom/chrome-webidl/BrowsingContext.webidl`). → (b)
- `SimpleGestureEvent` + `widget/SwipeTracker`: consumer-configurable swipe
  thresholds for space swiping. → (b)
- `MediaController.webidl` + `dom/media/mediacontrol/*` +
  `HTMLMediaElement`/`MediaStatusManager`: position-state + PiP flag APIs. → (b)
- `servo/*`: `-moz-bool-pref` media-feature (CSS conditioned on a pref,
  chrome/UA only). → **(d)** — replaced by whatever styling system the new UI
  uses.
- `widget/` — SwipeTracker, ScrollbarDrawing colors, Theme Colors,
  `VibrancyManager`/`nsCocoaWindow`/`nsCocoaUtils` (macOS acrylic/vibrancy),
  `nsDragService` cocoa, GTK drag session. → (b) platform polish.
- `xpfe/appshell/AppWindow.cpp` — `verticalTabs` window flag. → (b)
- `docshell/nsAboutRedirector` — `about:credits` → zen site. → (c)/(a)
- `image/AutoRestoreSVGState`, `accessible/*`, `editor/HTMLAbsPositionEditor`,
  `devtools/*` — small fixes. → (b)
- `modules/libpref/init/StaticPrefList` — `#include zen-static-prefs.inc`
  (all `zen.*` prefs). → (c)
- `services/sync/service-sys-mjs` — registers the Spaces sync engine. → (d)
- `build/moz-configure`, `lto-pgo-configure`, `security/hardenedruntime` — (c)
- `external-patches/` — vendored upstream fixes: backdrop-filter-reads-web-content,
  HEVC stutter fix, expose tiled attr, native macOS popovers, no-liquid-glass
  icon, cert-check override (dev), issue_14990, librewolf UA. → not portable,
  re-evaluate against Chromium (mostly N/A).
- `testing/`, `python/mozlint`, root eslint/stylelint patches — (c) tooling.

### Repo-level build/config (outside `src/`)

- `surfer.json` — surfer overlay config: engine = Firefox 158.0.1, brands
  (release/twilight), update MARs per-platform.
- `prefs/zen/*.yaml` — ~20 YAML pref registries (generated into
  `zen-static-prefs.inc`).
- `configs/{branding,common,linux,macos,windows,dumps}` — per-OS build configs,
  branding.
- `locales/` — ~50 locales, Fluent `.ftl` files for all Zen strings.
- `build/` — AppDir, flatpak, signing, windows packaging scripts.
- `tools/` + `scripts/` — ffprefs (Rust pref generator), update scripts, test
  runner.

---

## 3. Categorized complexity summary

| Module | Cat | Size | Est |
|---|---|---|---|
| Workspaces (spaces) | a+b+d | 9.1K | **XL** |
| Split view | a (+multi-WebContents compositing) | 3.1K | **XL** |
| Glance preview | a + IPC | 2.6K | L |
| Compact mode | a + small native | 2.0K | M |
| Mods (chrome CSS mods) | d + marketplace web | 1.3K | L |
| Tab folders | a | 3.1K | M |
| Live folders (RSS/GitHub) | a | 2.1K | M |
| Keyboard shortcuts | a | 1.7K | M |
| Media controls | a + small engine API | 1.3K | M |
| Download animation | a | 0.6K | S |
| Share (server + native share) | a + service | 1.5K | M |
| Sync (Weave engine) | d | 2.0K | **XL** |
| Session store + window sync | b+d | 2.9K | **XL** |
| Space routing | a + nav interception | 1.7K | L |
| URL bar (floating + providers + site panel) | a+d | 2.8K | L |
| Library panel | a + small native scan | 8.4K | L |
| Boosts (per-site color transform) | **b** deep engine | 7.9K | **XL** |
| Zap (element remover) | a + actors | incl. boosts | M |
| Vertical tabs/essentials/pinned/rename | a | 3.0K | L |
| Tab-strip drag & drop | a + small native | 2.4K | L |
| Common infra/theme/startup | a (+d if UI not CSS) | 8.8K | L |
| Welcome/onboarding | a + ext-install | 1.4K | M |
| Native share/haptics | b | 0.3K | S |
| Window drag from content | b | 0.5K | M |
| Vendor (motion.js), fonts, images | c | — | S |
| @types | d (regen) | 80K | S |
| tests | c/d | 116K | L |
| Surfer/build/branding/locales/prefs | c | — | M |

Legend: **S** ≈ ≤1 person-week · **M** 1–3 · **L** 3–6 · **XL** >6 —
rough human-engineering estimates for a ported equivalent, including testing.

Total `src/zen/` product code ≈ **52K LOC** (excluding `tests/` and `@types/`).

---

## 4. Hard Gecko/XPCOM dependencies (things with no direct Chromium analog)

| Dependency | Where it's used | Chromium replacement strategy |
|---|---|---|
| **XUL document + XUL widgets** (`MozXULElement`, `parseXULToFragment`, `<tabbrowser>`, `<tabbox>`, `<toolbox>`, `<xul:menu/popup>`) | Every UI module — workspaces, folders, compact-mode, glance overlay markup, panels | Entire UI layer must be rebuilt (Views C++ UI or WebUI/TypeScript). No mechanical port. |
| **`<browser type="content">` + remoteness** | Glance, split-view, media | `WebContents` hosted in a custom view / second `WebContentsView`; remoteness model differs (site isolation) |
| **gBrowser / tab DOM model** (tab elements, `zen-workspace-id` attrs, `TabOpen/TabClose/...` events, `tabContainer`, `tabpanels`, `AsyncTabSwitcher`) | Nearly all features | `TabStripModel` + custom tab-strip UI; event equivalents in browser observers |
| **SessionStore internals** (`SessionStore.sys.mjs`, `TabState`, `TabStateCache`, `TabStateFlusher`, `SessionSaver`) | sessionstore, live-folders, share, window sync | Chromium `sessions`/tab restore service + custom serialization for zen state |
| **JSWindowActor/JSActor IPC** | glance, mods marketplace, window drag, boosts | WebContents→browser messaging: `mojo` IPC / `WebContentsObserver` / extension messaging |
| **XPCOM components/IDL** (`nsIZen*`, `Cc[...]` contracts) | mods backend, mouse tracker, drag/drop, window drag, share, library files | Plain C++ services in `//chrome/browser` (no IDL needed) |
| **Firefox Sync (Weave) engine framework** | `sync/` | No analog — bespoke sync backend or adapt `components/sync` |
| **Places DB** (`PlacesUtils`, `zen_bookmarks_workspaces` tables) | space bookmarks | Chromium `history`/bookmark model + own SQL or JSON store |
| **ContextualIdentityService (containers)** | space routing, workspaces | No analog — emulate with profiles/partitions or custom storage partitioning |
| **CustomizableUI** | sidebar areas, library widget | Chromium toolbar model (rebuild) |
| **Urlbar provider framework** | urlbar/ | `AutocompleteProvider`/`OmniboxController` APIs |
| **WebExtensions APIs** (`AddonManager`, ext-*) | welcome (uBlock), extensions panel tweaks | Chrome Web Store install flow; `extensions/` API differs |
| **Servo/Stylo style system** | boosts color transforms | Blink style/`ComputedStyle` hooks or a cc/renderer color-filter |
| **CSS-driven chrome styling + stylesheet service** | mods, theme system | Chromium chrome UI isn't CSS (Views). If new UI is WebUI/HTML it becomes directly portable CSS |
| **`.ftl` Fluent l10n** | all strings | Chromium `localized_strings` (GRIT/IDS) or WebUI string APIs |
| **jar.mn/chrome:// packaging + category manifests** | all modules | Chromium `BUILD.gn` resources/pak; startup wiring via `ChromeBrowserMainParts` |
| **`mach`/surfer overlay + moz.build** | whole build | `gn`/`ninja`; the patch-overlay concept disappears — direct edits |
| **PGO training tour** (`ZenPGOTraining`) | build perf | n/a (drop or re-target) |
| **`moz-src://`/`resource://` module URIs, `ChromeUtils.importESModule`** | all sys.mjs | WebUI modules / TypeScript / C++ services |
| **`userContextId` on tabs/browsers** | space routing, glance | Storage partitioning / `SiteInstance` config or profile scoping |
| **`PrivateBrowsingUtils`, `E10SUtils`** | everywhere | `IncognitoProfile`, site-isolation checks |
| **Native `SwipeTracker`/`SimpleGestureEvent`** | spaces swipe | Chromium gesture recognizer (`ui/events`) — needs engine-side tunables |
| **`nsPresContext`/`BrowsingContext` fields for boosts** | boosts | `WebContents`/`RenderFrameHost` user-data + blink-side style hook |
