#!/usr/bin/env bash
# Produce compact evidence when all twelve confirmation arms have finished.
set -euo pipefail
if [[ $# != 1 ]]; then
  echo "usage: $0 SUPERVISOR_PID" >&2
  exit 2
fi

repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"
root=artifacts/policy-stability-confirmation
inputs=artifacts/policy-confirmation-inputs
analysis="$root/analysis"
supervisor_pid=$1
python="$HOME/Srv/aic-rust-learning-20260915/.venv/bin/python"
mkdir -p "$analysis"

while :; do
  complete=0
  for stream in s1 s2 s3; do
    for arm in a b c d; do
      observer="$root/$stream/arm-$arm/wandb-observer.json"
      if [[ -f "$observer" ]] && "$python" - "$observer" <<'PY'
import json,sys
status=json.load(open(sys.argv[1]))
raise SystemExit(0 if status.get('status') == 'finished'
                 and status.get('remote_confirmed_step', -1) >= 7 else 1)
PY
      then complete=$((complete + 1)); fi
    done
  done
  if [[ "$complete" == 12 ]]; then break; fi
  state=$(ps -p "$supervisor_pid" -o stat= 2>/dev/null || true)
  if [[ -z "$state" || "$state" == Z* ]]; then
    echo "supervisor ended with only $complete of 12 acknowledged arms" >&2
    exit 1
  fi
  sleep 120
done

for stream in s1 s2 s3; do
  "$python" tools/factory-edit-pilot/analyze_hierarchical_stability_confirmation.py \
    --confirmation "$root" --inputs "$inputs" --stream "$stream" \
    --output "$analysis/$stream.json"
done
"$python" tools/factory-edit-pilot/summarize_hierarchical_stability_confirmation.py \
  --evidence-dir "$analysis" --output "$analysis/summary.json"
echo "confirmation_analysis_complete=$analysis"
