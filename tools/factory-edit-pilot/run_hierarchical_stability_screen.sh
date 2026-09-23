#!/usr/bin/env bash
# Sequential four-arm screen, with a separate resumable W&B observer per arm.
set -euo pipefail

if [[ $# != 2 ]]; then
  echo "usage: $0 WANDB_ENV_FILE ACKNOWLEDGED_SMOKE_RUN_ID" >&2
  exit 2
fi
wandb_env=$1
readiness_id=$2
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"

export LIBTORCH="$HOME/Srv/aic-rust-learning-20260915/.venv/lib/python3.12/site-packages/torch"
export LIBTORCH_CXX11_ABI=1
export LD_LIBRARY_PATH="$LIBTORCH/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
python="$HOME/Srv/aic-rust-learning-20260915/.venv/bin/python"
base=docs/benchmarks/hierarchical-policy-stability
inputs=artifacts/policy-stability-inputs
root=artifacts/policy-stability-screen
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
if run.lastHistoryStep < 0:
    raise RuntimeError('smoke readiness record has not reached W&B')
print(f'wandb_readiness_acknowledged_step={run.lastHistoryStep}', flush=True)
PY

for arm in a b c d; do
  output="$root/arm-$arm"
  if [[ -e "$output" ]]; then
    echo "refusing an existing screen output: $output" >&2
    exit 1
  fi
  echo "starting_arm=$arm" >&2
  target/release/aic-cli learning train-hierarchical-curriculum \
    --config "$base/curriculum-$arm.json" \
    --search-config "$base/search.json" \
    --generation-config "$base/generation-$arm.json" \
    --sampling docs/benchmarks/direct-constructive-full-graphs/sampling.json \
    --heldout-panel "$inputs/diagnostic-panel.json" \
    --initial-lower-output "$inputs/initial-lower" \
    --initial-outer-output "$inputs/initial-outer" \
    --catalog-root data/game/normalized --output "$output" --device cuda:0 \
    > "$root/logs/arm-$arm.log" 2>&1 &
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
    echo "trainer did not publish arm $arm state" >&2
    kill "$trainer" 2>/dev/null || true
    exit 1
  fi

  "$python" tools/factory-edit-pilot/hierarchical_curriculum_tracking.py \
    --output "$output" --env-file "$wandb_env" --poll-seconds 2 \
    > "$root/logs/arm-$arm-observer.log" 2>&1 &
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
    echo "W&B observer failed readiness for arm $arm" >&2
    kill "$trainer" 2>/dev/null || true
    wait "$observer" || true
    exit 1
  fi

  if ! wait "$trainer"; then
    echo "trainer failed for arm $arm" >&2
    kill "$observer" 2>/dev/null || true
    wait "$observer" || true
    exit 1
  fi
  wait "$observer"

  "$python" - "$output" "$inputs" "$arm" <<'PY'
import hashlib,json,pathlib,sys
output,inputs,arm=pathlib.Path(sys.argv[1]),pathlib.Path(sys.argv[2]),sys.argv[3]
state=json.loads((output/'state.json').read_text())
observer=json.loads((output/'wandb-observer.json').read_text())
assert state['completed_generations']==8 and observer['status']=='finished'
lower=hashlib.sha256((inputs/'initial-lower/state.safetensors').read_bytes()).hexdigest()
for generation in range(8):
    directory=output/'generations'/f'{generation:06}'
    expected=inputs/'common-panels'/f'{generation:06}.json'
    actual=directory/'training-panel.json'
    assert hashlib.sha256(actual.read_bytes()).digest()==hashlib.sha256(expected.read_bytes()).digest()
    report=json.loads((directory/'training/report.json').read_text())
    assert report['updates']==3200 and report['outer_states_per_epoch']==256
    assert report['lower_checkpoint_sha256']==lower and not report['train_lower']
    assert report['train_failed_policy']==(arm in 'ab')
    if generation==1:
        assert (report['replay_root_states']>0)==(arm in 'bd')
print(f'arm_complete={arm} remote_confirmed_step={observer["remote_confirmed_step"]}',flush=True)
PY
done
