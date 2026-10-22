# Chromium Port — Implementation Plan

Status: DRAFT — synthesized from the research documents in this directory.
Read `FEATURE-INVENTORY.md`, `CHROMIUM-PORT-ARCHITECTURE.md`, and
`PRIOR-ART-AND-ADBLOCK.md` first.

## Guiding constraints

- **UI parity, no redesign.** Re-implement Zen's UX on Chromium; do not
  re-invent it. Zen's visual language (sidebar, workspaces, compact mode)
  is reproduced feature-for-feature.
- **Brave's adblocker.** Network + cosmetic filtering via `adblock-rust`,
  delivered by the Brave base (shields engine, filter lists, cosmetic
  scriptlets) rather than an extension (MV3 content-filtering limits).
- **Gecko stays in this repo.** The port is a *new* product line; the
  Firefox-based Zen keeps living in `src/`. Port assets live under
  `chromium-port/`; the Chromium-side fork lives in its own repo(s)
  (see architecture doc).

## Phases

| Phase | Scope | Depends on |
|-------|-------|------------|
| 0 | Build infrastructure: fork setup, pinned Chromium/Brave versions, patch-series tooling, first clean `npm run build` | — |
| 1 | Sidebar / vertical tabs parity: layout, pinned tabs, essentials, drag & drop | 0 |
| 2 | Workspaces (Spaces): data model, UI, gradient/icons, per-space tab sets | 1 |
| 3 | Split view parity, glance (link preview overlay) | 1 |
| 4 | Compact mode, mods, keyboard shortcuts, urlbar, media, downloads, share | 1 |
| 5 | Session persistence parity (ZenSessionManager → Chromium session restore), spaces sync | 2 |
| 6 | Branding, prefs/profile migration tooling, release pipeline | all |

Ad-blocking is present from Phase 0 (inherited from the base) and is
re-skinned to Zen's settings surface in Phase 6.

## Per-phase details

_To be filled from the research docs once they land._
