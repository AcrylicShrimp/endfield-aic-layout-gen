#!/usr/bin/env bash
# Reconstruct prior exclusions and prepare one new disjoint diagnostic panel.
set -euo pipefail
if [[ $# != 1 ]]; then
  echo "usage: $0 AIC_CLI" >&2
  exit 2
fi
cli=$1
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"
root=artifacts/update-budget-probe/inputs
if [[ -e "$root" ]]; then
  echo "refusing existing inputs: $root" >&2
  exit 1
fi
mkdir -p "$root"
sampling=docs/benchmarks/direct-constructive-full-graphs/sampling.json
selector=docs/benchmarks/hierarchical-policy-stability/select-diagnostic.py

panel() {
  local output=$1 cases=$2 seed=$3 namespace=$4
  mkdir -p "$(dirname "$output")"
  "$cli" learning prepare-hierarchical-panel \
    --sampling "$sampling" --catalog-root data/game/normalized \
    --output "$output" --partition training --cases "$cases" \
    --seed "$seed" --namespace "$namespace" \
    --min-facilities 2 --max-facilities 2 --max-terminals 6 \
    --max-connections 6 --max-turns 10000 --max-extension 32 >/dev/null
}

for stream in s1 s2 s3; do
  case "$stream" in
    s1) seed=20261210 ;;
    s2) seed=20261230 ;;
    s3) seed=20261250 ;;
  esac
  panel "$root/training/$stream.json" 32 "$seed" "hierarchical-update-budget-$stream-v1"
done

for generation in {0..7}; do
  printf -v number '%06d' "$generation"
  panel "$root/prior-screen-training/$number.json" 32 "$((20260930+generation))" \
    "hierarchical-policy-stability-v1-g$number"
done
panel "$root/prior-screen-candidates.json" 256 20261000 \
  hierarchical-policy-stability-diagnostic-v1
python3 "$selector" --training-panels "$root/prior-screen-training" \
  --candidates "$root/prior-screen-candidates.json" \
  --output "$root/prior-screen-diagnostic.json"
test "$(sha256sum "$root/prior-screen-diagnostic.json" | cut -d' ' -f1)" = \
  0fc86bb56d0ec0809b782c850f81a02a954a14416c2e83742f28a5a73c62e29f

for stream in s1 s2 s3; do
  case "$stream" in
    s1) base=20261010 ;;
    s2) base=20261030 ;;
    s3) base=20261050 ;;
  esac
  for generation in {0..7}; do
    printf -v number '%06d' "$generation"
    panel "$root/prior-confirmation-training/$stream/$number.json" 32 \
      "$((base+generation))" "hierarchical-policy-confirmation-$stream-v1-g$number"
  done
done
panel "$root/prior-confirmation-candidates.json" 512 20261100 \
  hierarchical-policy-confirmation-diagnostic-v1
python3 "$selector" --training-panels "$root/prior-confirmation-training" \
  --expected-panels 24 --candidates "$root/prior-confirmation-candidates.json" \
  --candidate-cases 512 --exclude-panel "$root/prior-screen-diagnostic.json" \
  --output "$root/prior-confirmation-diagnostic.json"
test "$(sha256sum "$root/prior-confirmation-diagnostic.json" | cut -d' ' -f1)" = \
  e452abcc69d48cda568c714f53c44e91547a54e94eb25ef1cd8a3efc8034de5a

panel "$root/diagnostic-candidates.json" 512 20261300 \
  hierarchical-update-budget-diagnostic-v1
python3 "$selector" --training-panels "$root/training" \
  --expected-panels 3 --candidates "$root/diagnostic-candidates.json" \
  --candidate-cases 512 \
  --exclude-panel "$root/prior-screen-diagnostic.json" \
  --exclude-panel "$root/prior-confirmation-diagnostic.json" \
  --output "$root/diagnostic.json"

source="$HOME/Srv/aic-gumbel-curriculum-20260922/artifacts/hierarchical-curriculum-v1/generations/000087/training"
cp -a "$source/lower" "$root/initial-lower"
cp -a "$source/outer" "$root/initial-outer"
cp docs/benchmarks/hierarchical-policy-stability/outer-initial.json \
  "$root/initial-outer/config.json"
test "$(sha256sum "$root/initial-lower/state.safetensors" | cut -d' ' -f1)" = \
  353d560842da6c4270605da552a04adc8d96411512d8d3b847625cf9c502dea9
test "$(sha256sum "$root/initial-outer/state.safetensors" | cut -d' ' -f1)" = \
  6a1f61578aa29a9e80d90ab7805bfec2ef8e543bd24fd9db90441fbb5d724520
echo "prepared_inputs=$root"
