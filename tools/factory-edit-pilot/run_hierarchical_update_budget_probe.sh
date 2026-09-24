#!/usr/bin/env bash
# Launch an independently observed paired update-budget matrix.
set -euo pipefail
if [[ $# != 2 ]]; then
  echo "usage: $0 WANDB_ENV_FILE EXPERIMENT_COMMIT" >&2
  exit 2
fi
wandb_env=$1
experiment_commit=$2
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"

export LIBTORCH="$HOME/Srv/aic-rust-learning-20260915/.venv/lib/python3.12/site-packages/torch"
export LIBTORCH_CXX11_ABI=1
export LD_LIBRARY_PATH="$LIBTORCH/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
python="$HOME/Srv/aic-rust-learning-20260915/.venv/bin/python"
root=artifacts/update-budget-probe

"$python" tools/factory-edit-pilot/hierarchical_update_budget_probe.py prepare \
  --output "$root" --experiment-commit "$experiment_commit"

"$python" tools/factory-edit-pilot/hierarchical_update_budget_tracking.py \
  --output "$root" --env-file "$wandb_env" > "$root/observer.log" 2>&1 &
observer=$!
ready=0
for _ in {1..90}; do
  if [[ -f "$root/wandb-observer.json" ]] && "$python" - "$root/wandb-observer.json" <<'PY'
import json,sys
status=json.load(open(sys.argv[1]))
raise SystemExit(0 if status['status'] in ('ready', 'running') and status['remote_confirmed_step']>=0 else 1)
PY
  then ready=1; break; fi
  if ! kill -0 "$observer" 2>/dev/null; then break; fi
  sleep 1
done
if [[ "$ready" != 1 ]]; then
  echo "W&B readiness record was not acknowledged" >&2
  kill "$observer" 2>/dev/null || true
  wait "$observer" || true
  exit 1
fi
echo "wandb_readiness_acknowledged=1" >&2

"$python" tools/factory-edit-pilot/hierarchical_update_budget_probe.py run \
  --output "$root" > "$root/runner.log" 2>&1 &
runner=$!
if ! wait "$runner"; then
  echo "update-budget runner failed" >&2
  kill "$observer" 2>/dev/null || true
  wait "$observer" || true
  exit 1
fi
wait "$observer"
"$python" - "$root" <<'PY'
import json,sys
from pathlib import Path
root=Path(sys.argv[1])
progress=json.loads((root/'progress.json').read_text())
observer=json.loads((root/'wandb-observer.json').read_text())
assert progress['status']=='complete' and len(progress['records'])==12
assert observer['status']=='finished' and observer['remote_confirmed_step']>=12
print(f"matrix_complete=12 remote_confirmed_step={observer['remote_confirmed_step']}")
PY
