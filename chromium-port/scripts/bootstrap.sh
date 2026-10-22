#!/usr/bin/env bash
# bootstrap.sh — fetch the Brave/Chromium source tree that the Zen port builds on.
#
# Produces a gclient checkout:
#   <work>/src        — brave/chromium at the tag pinned by brave-core's package.json
#   <work>/src/brave  — our fork of brave-core (Jackson57279/zen-core)
#
# Usage:
#   ZEN_CORE_REPO=git@github.com:Jackson57279/zen-core.git \
#   ZEN_CORE_BRANCH=1.82.x \
#   ./bootstrap.sh <workdir>
#
# Requires ~70 GB free disk and pnpm/node per brave-core's devEngines.
# See docs/CHROMIUM-PORT-ARCHITECTURE.md §2 for the mechanics this drives.

set -euo pipefail

WORKDIR="${1:?usage: bootstrap.sh <workdir>}"
ZEN_CORE_REPO="${ZEN_CORE_REPO:-git@github.com:Jackson57279/zen-core.git}"
ZEN_CORE_BRANCH="${ZEN_CORE_BRANCH:-master}"   # pin a brave stable branch, not master

# Rebuildable checkouts: keep a shared git cache so re-syncs are cheap.
export GIT_CACHE_PATH="${GIT_CACHE_PATH:-${HOME}/.git_cache}"
export DEPOT_TOOLS_UPDATE=0
export GSUTIL_ENABLE_LUCI_AUTH=0
export PYTHONUNBUFFERED=1

mkdir -p "$WORKDIR/src"
cd "$WORKDIR"

# brave-core is the *build driver*: it fetches Chromium, applies src/brave/patches,
# downloads toolchains. It must sit at src/brave before `pnpm run init`.
if [ ! -d src/brave ]; then
  git clone --branch "$ZEN_CORE_BRANCH" --single-branch "$ZEN_CORE_REPO" src/brave
fi

cd src/brave
pnpm install --frozen-lockfile

# gclient sync of ~240 repos / ~60GB. Long. Re-run `pnpm run sync` to advance.
pnpm run init

# Apply any raw patches from this spec repo that chromium_src/plaster cannot
# express (see chromium-port/patches/series). Normally empty — prefer overrides.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
"$SCRIPT_DIR/apply_series.sh" "$WORKDIR"

echo "Checkout ready. Next: cd $WORKDIR/src/brave && pnpm run build Component"
