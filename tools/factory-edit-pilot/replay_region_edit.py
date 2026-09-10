"""Replay one retained action through the normal native transaction boundary."""
import argparse
import hashlib
import json
from pathlib import Path

from native_factory_runtime import FactoryRuntime, ROOT
from random_factory_generation import write


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('request', 'witness', 'action', 'catalogs', 'library', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--work', type=int, default=5000)
    parser.add_argument('--repair', type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    runtime = FactoryRuntime(args.library, args.catalogs)
    action = json.loads(args.action.read_text())
    limits = json.loads(args.repair.read_text()) if args.repair else None
    episode = runtime.open_episode(dict(request=str(args.request.resolve()), initial=str(args.witness.resolve())),
                                   1, 120, args.work, repair=limits)
    view = episode.candidates()
    domain = json.loads(episode.diagnostic_domain_json())
    matches = [i for i, a in enumerate(domain['actions']) if a == action]
    if len(matches) != 1:
        raise ValueError(f'region-replay.action-domain: expected one matching action from {args.action}, found {len(matches)}')
    result = episode.step(view['handle'], matches[0])
    write(args.output / 'step.json', result)
    write(args.output / 'current.json', json.loads(episode.export_layout('current')))
    write(args.output / 'best.json', json.loads(episode.export_layout('best')))
    write(args.output / 'provenance.json', dict(schema_version=1, runtime=runtime.metadata,
        inputs={str(p.resolve()): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in (args.request, args.witness, args.action, args.catalogs)}, work=args.work, repair=limits))
    print(json.dumps(dict(status=result['status'], score=result['current_score'], work=result['route_expansions'],
        attempts=len(result.get('region', {}).get('attempts', [])), output=str(args.output.resolve()))))


if __name__ == '__main__':
    main()
