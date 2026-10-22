# Chromium Port

Working directory for the effort to re-base Zen Browser on Chromium with
Brave's ad-block engine (`adblock-rust`), targeting feature and UI parity with
the current Gecko build — no UI redesign.

## Why a port, not a patch

Zen's `src/` tree is a [surfer](https://github.com/zen-browser/surfer) overlay
applied on top of mozilla-central at build time. The front-end code in
`src/zen/` (workspaces, split view, glance, compact mode, sidebar tabs, mods)
is XUL/CSS/JS running on Gecko internals (XPCOM, sessionstore, `browser/`
actors). None of it executes on Chromium's stack — Blink instead of Gecko,
Views/WebUI instead of XUL, `content/` actors instead of `browser/actors`.

Porting therefore means **re-implementing each feature against Chromium**,
guided by the inventory in `docs/FEATURE-INVENTORY.md`.

## Documents

| Document | Contents |
|----------|----------|
| `docs/FEATURE-INVENTORY.md` | Every Zen feature, its Gecko hooks, port category, and complexity estimate. |
| `docs/CHROMIUM-PORT-ARCHITECTURE.md` | Recommended base (Brave fork vs. Chromium + adblock-rust), build system, patch management. |
| `docs/PRIOR-ART-AND-ADBLOCK.md` | Existing Chromium-based attempts, adblock-rust integration deep-dive, licensing. |
| `docs/PORT-PLAN.md` | Phased implementation plan synthesized from the three documents above. |

## Layout

```
chromium-port/
  docs/       # port specification
  patches/    # patch series applied to the Chromium/Brave source tree (by phase)
  scripts/    # checkout / patch-application / build helpers
```

## Status

Research + spec phase. See `docs/PORT-PLAN.md` for the current plan.
