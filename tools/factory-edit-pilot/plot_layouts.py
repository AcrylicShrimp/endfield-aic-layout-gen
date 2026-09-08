"""Render admitted recorded factory layouts directly, with independent validity checks."""

import argparse
import json
from pathlib import Path

from evidence import provenance, write
from mastery_policy_selection import digest, read
from native import Runtime
from plots import render_layouts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case', type=Path, required=True)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--initial', action='store_true')
    parser.add_argument('--layout', action='append', nargs=2, metavar=('LABEL', 'PATH'), default=[])
    parser.add_argument('--caption')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        raise ValueError(f'plots.output: {output}: choose a fresh directory')
    root = Path(__file__).resolve().parents[2]
    runtime = Runtime(root, args.case.resolve(), read(args.config), args.native_library.resolve())
    layouts, inputs = [], []
    if args.initial:
        layouts.append((json.loads(runtime.new_engine().export_layout_json('best')), 'Original valid factory'))
    for label, value in args.layout:
        path = Path(value).resolve()
        layouts.append((read(path), label))
        inputs.append({'label': label, 'path': str(path), 'sha256': digest(path)})
    if not layouts:
        raise ValueError('plots.input: supply --initial or at least one --layout LABEL PATH')
    output.mkdir(parents=True)
    figures = render_layouts(output, runtime, layouts, args.caption)
    manifest = {'schema_version': 1, 'status': 'complete', 'inputs': inputs,
                'provenance': provenance(runtime, args.config.resolve()),
                'figures': {key: {'path': value, 'sha256': digest(Path(value))} for key, value in figures.items()}}
    write(output/'view.json', manifest)
    print(json.dumps({'status': 'complete', **figures}))


if __name__ == '__main__':
    main()
