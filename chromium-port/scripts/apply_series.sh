#!/usr/bin/env bash
# apply_series.sh — apply chromium-port/patches (quilt series) onto a checkout.
#
# Applies each .patch listed in patches/series against <workdir>/src (the
# Chromium tree) — raw patches only ever exist for changes that cannot be
# expressed as a src/brave/chromium_src override or a rewrite/ plaster rule.
#
# Usage: apply_series.sh <workdir>   # workdir contains src/ + src/brave

set -euo pipefail

WORKDIR="${1:?usage: apply_series.sh <workdir>}"
SERIES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/patches"
SERIES="$SERIES_DIR/series"

[ -f "$SERIES" ] || { echo "no patches/series — nothing to apply"; exit 0; }

cd "$WORKDIR/src"
grep -Ev '^\s*(#|$)' "$SERIES" | while read -r patch; do
  echo "applying $patch"
  git apply --check "$SERIES_DIR/$patch"
  git apply "$SERIES_DIR/$patch"
done
echo "series applied ($(grep -cEv '^\s*(#|$)' "$SERIES") patches)"
