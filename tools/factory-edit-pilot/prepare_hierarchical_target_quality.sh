#!/usr/bin/env bash
# Prepare three frozen target-audit panels and the two search contracts.
set -euo pipefail
if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: $0 AIC_CLI [OUTPUT_ROOT]" >&2
  exit 2
fi
cli=$1
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"
root=${2:-artifacts/target-quality-audit}
if [[ -e "$root" ]]; then
  echo "refusing existing audit root: $root" >&2
  exit 1
fi
mkdir -p "$root/inputs/panels" "$root/inputs/candidates" "$root/inputs/models"

for stream in s1 s2 s3; do
  case "$stream" in
    s1) seed=20261410 ;;
    s2) seed=20261430 ;;
    s3) seed=20261450 ;;
  esac
  "$cli" learning prepare-hierarchical-panel \
    --sampling docs/benchmarks/direct-constructive-full-graphs/sampling.json \
    --catalog-root data/game/normalized \
    --output "$root/inputs/candidates/$stream.json" \
    --partition training --cases 256 --seed "$seed" \
    --namespace "hierarchical-target-quality-$stream-v1" \
    --min-facilities 2 --max-facilities 2 --max-terminals 6 \
    --max-connections 6 --max-turns 10000 --max-extension 32 >/dev/null
done
python3 - "$root/inputs" <<'PY'
import hashlib
import json
from pathlib import Path
import sys

root = Path(sys.argv[1])
seen = set()
for stream in ("s1", "s2", "s3"):
    panel = json.loads((root / "candidates" / f"{stream}.json").read_text())
    selected = []
    for case in panel["cases"]:
        encoded = json.dumps(case["problem"], sort_keys=True, separators=(",", ":"))
        identity = hashlib.sha256(encoded.encode()).hexdigest()
        if identity not in seen:
            selected.append(case)
            seen.add(identity)
        if len(selected) == 16:
            break
    if len(selected) != 16:
        raise RuntimeError(f"only {len(selected)} distinct cases for {stream}")
    panel["cases"] = selected
    (root / "panels" / f"{stream}.json").write_text(json.dumps(panel, indent=2) + "\n")
print(f"selected_panels=3 distinct_problems={len(seen)}")
PY

source="$HOME/Srv/aic-gumbel-curriculum-20260922/artifacts/hierarchical-curriculum-v1/generations/000087/training"
cp -a "$source/lower" "$root/inputs/models/lower"
cp -a "$source/outer" "$root/inputs/models/outer-128"
cp -a "$source/outer" "$root/inputs/models/outer-512"
cp docs/benchmarks/hierarchical-policy-stability/outer-initial.json \
  "$root/inputs/models/outer-128/config.json"
cp docs/benchmarks/hierarchical-target-quality/outer-512.json \
  "$root/inputs/models/outer-512/config.json"
test "$(sha256sum "$root/inputs/models/lower/state.safetensors" | cut -d' ' -f1)" = \
  353d560842da6c4270605da552a04adc8d96411512d8d3b847625cf9c502dea9
test "$(sha256sum "$root/inputs/models/outer-128/state.safetensors" | cut -d' ' -f1)" = \
  6a1f61578aa29a9e80d90ab7805bfec2ef8e543bd24fd9db90441fbb5d724520
cmp "$root/inputs/models/outer-128/state.safetensors" \
  "$root/inputs/models/outer-512/state.safetensors"
echo "prepared_target_quality_inputs=$root/inputs"
