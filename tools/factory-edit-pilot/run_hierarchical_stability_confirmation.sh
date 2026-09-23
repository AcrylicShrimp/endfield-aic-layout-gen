#!/usr/bin/env bash
# Three independent streams, four paired arms each; fail closed on any guard.
set -euo pipefail

if [[ $# != 2 && !( $# == 3 && $3 == --resume ) ]]; then
  echo "usage: $0 WANDB_ENV_FILE ACKNOWLEDGED_SCREEN_RUN_ID [--resume]" >&2
  exit 2
fi
wandb_env=$1
readiness_id=$2
resume=${3:-}
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"

export LIBTORCH="$HOME/Srv/aic-rust-learning-20260915/.venv/lib/python3.12/site-packages/torch"
export LIBTORCH_CXX11_ABI=1
export LD_LIBRARY_PATH="$LIBTORCH/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
python="$HOME/Srv/aic-rust-learning-20260915/.venv/bin/python"
base=docs/benchmarks/hierarchical-policy-stability
configs=docs/benchmarks/hierarchical-policy-confirmation
inputs=artifacts/policy-confirmation-inputs
root=artifacts/policy-stability-confirmation
mkdir -p "$root/logs"

"$python" - "$wandb_env" "$readiness_id" <<'PY'
import sys
sys.path.insert(0, 'tools/factory-edit-pilot')
from ppo_harness_environment import load_environment, tracking_destination
load_environment(sys.argv[1])
import wandb
destination = tracking_destination(required=True)
run = wandb.Api(timeout=15).run(
    f"{destination['entity']}/{destination['project']}/{sys.argv[2]}")
if run.lastHistoryStep < 7:
    raise RuntimeError('screen record has not reached W&B step 7')
print(f'wandb_readiness_acknowledged_step={run.lastHistoryStep}', flush=True)
PY

verify_arm() {
  "$python" - "$1" "$inputs" "$2" "$3" <<'PY'
import hashlib,json,pathlib,sys
output,inputs,stream,arm=pathlib.Path(sys.argv[1]),pathlib.Path(sys.argv[2]),sys.argv[3],sys.argv[4]
state=json.loads((output/'state.json').read_text())
observer=json.loads((output/'wandb-observer.json').read_text())
assert state['completed_generations']==8 and observer['status']=='finished'
assert observer['remote_confirmed_step']>=7
lower=hashlib.sha256((inputs/'initial-lower/state.safetensors').read_bytes()).hexdigest()
for generation in range(8):
    directory=output/'generations'/f'{generation:06}'
    expected=inputs/'common-panels'/stream/f'{generation:06}.json'
    actual=directory/'training-panel.json'
    assert hashlib.sha256(actual.read_bytes()).digest()==hashlib.sha256(expected.read_bytes()).digest()
    report=json.loads((directory/'training/report.json').read_text())
    assert report['updates']==3200 and report['outer_states_per_epoch']==256
    assert report['lower_checkpoint_sha256']==lower and not report['train_lower']
    assert report['train_failed_policy']==(arm in 'ab')
    if generation==1:
        assert (report['replay_root_states']>0)==(arm in 'bd')
print(f'arm_complete={stream}-{arm} remote_confirmed_step={observer["remote_confirmed_step"]}',flush=True)
PY
}

for stream in s1 s2 s3; do
  for arm in a b c d; do
    output="$root/$stream/arm-$arm"
    if [[ -e "$output" ]]; then
      if [[ "$resume" != --resume ]]; then
        echo "refusing an existing confirmation output: $output" >&2
        exit 1
      fi
      verify_arm "$output" "$stream" "$arm"
      echo "reused_completed_arm=$stream-$arm" >&2
      continue
    fi
    mkdir -p "$root/$stream"
    echo "starting_stream=$stream arm=$arm" >&2
    target/release/aic-cli learning train-hierarchical-curriculum \
      --config "$configs/curriculum-$stream-$arm.json" \
      --search-config "$base/search.json" \
      --generation-config "$base/generation-$arm.json" \
      --sampling docs/benchmarks/direct-constructive-full-graphs/sampling.json \
      --heldout-panel "$inputs/diagnostic-panel.json" \
      --initial-lower-output "$inputs/initial-lower" \
      --initial-outer-output "$inputs/initial-outer" \
      --catalog-root data/game/normalized --output "$output" --device cuda:0 \
      > "$root/logs/$stream-$arm.log" 2>&1 &
    trainer=$!

    ready=0
    for _ in {1..60}; do
      if [[ -f "$output/state.json" ]]; then ready=1; break; fi
      if ! kill -0 "$trainer" 2>/dev/null; then
        wait "$trainer"
        exit 1
      fi
      sleep 1
    done
    if [[ "$ready" != 1 ]]; then
      echo "trainer did not publish $stream $arm state" >&2
      kill "$trainer" 2>/dev/null || true
      wait "$trainer" || true
      exit 1
    fi

    "$python" tools/factory-edit-pilot/hierarchical_curriculum_tracking.py \
      --output "$output" --env-file "$wandb_env" --poll-seconds 2 \
      > "$root/logs/$stream-$arm-observer.log" 2>&1 &
    observer=$!
    observer_ready=0
    for _ in {1..90}; do
      if [[ -f "$output/wandb-observer.json" ]] && "$python" - "$output/wandb-observer.json" <<'PY'
import json,sys
status=json.load(open(sys.argv[1]))
raise SystemExit(0 if status['status']=='running' else 1)
PY
      then observer_ready=1; break; fi
      if ! kill -0 "$observer" 2>/dev/null; then break; fi
      sleep 1
    done
    if [[ "$observer_ready" != 1 ]]; then
      echo "W&B observer failed readiness for $stream $arm" >&2
      kill "$trainer" 2>/dev/null || true
      wait "$trainer" || true
      wait "$observer" || true
      exit 1
    fi

    if ! wait "$trainer"; then
      echo "trainer failed for $stream $arm" >&2
      kill "$observer" 2>/dev/null || true
      wait "$observer" || true
      exit 1
    fi
    if ! wait "$observer"; then
      echo "observer_retry=$stream-$arm" >&2
      sleep 5
      "$python" tools/factory-edit-pilot/hierarchical_curriculum_tracking.py \
        --output "$output" --env-file "$wandb_env" --poll-seconds 2 \
        > "$root/logs/$stream-$arm-observer-retry.log" 2>&1 &
      observer=$!
      wait "$observer"
    fi

    verify_arm "$output" "$stream" "$arm"
  done
done
