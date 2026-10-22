# patches/

Raw `.patch` files applied to the Chromium source tree, in `series` order —
the GNU Quilt layout borrowed from ungoogled-chromium.

**Last resort only.** Per the port architecture (`docs/CHROMIUM-PORT-ARCHITECTURE.md`
§2), the preferred order of modification inside the brave-core fork is:

1. Plain code under `src/brave/zen/` (never touches upstream).
2. `src/brave/chromium_src/` overrides (file shadowing + macro injection).
3. `src/brave/rewrite/` plaster (regex substitutions generating patches).
4. Raw `.patch` — only what the above cannot express. Every raw patch is a
   rebase liability on each Chromium/Brave version hop.

## Rules (ungoogled-chromium discipline)

- `series` lists patches in apply order, one relative path per line; `#` comments.
- Patches are unified diffs, `-p1` paths relative to `<work>/src`.
- `core/` — must be re-validated on every Brave/Chromium bump.
- `extra/` — nice-to-have; may be dropped when it rots rather than blocking a rebase.
- Patches must apply with **zero fuzz** (`git apply --check` enforced by
  `scripts/apply_series.sh`).
- Keep non-patch data out of diffs (config, lists) — GN args belong in the
  brave-core fork's `build/config`.
