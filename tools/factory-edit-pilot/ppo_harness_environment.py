"""Read only the three W&B settings without shell evaluation or secret logging."""
import os
from pathlib import Path
import shlex

from ppo_harness_state import require

KEYS = ('WANDB_API_KEY', 'WANDB_ENTITY', 'WANDB_PROJECT')


def read_environment(path):
    values = {}
    for number, line in enumerate(Path(path).read_text().splitlines(), 1):
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        if line.startswith('export '):
            line = line[7:].lstrip()
        key, separator, raw = line.partition('=')
        key = key.strip()
        require(separator and key in KEYS and key not in values, 'environment-format',
                f'expected one supported W&B assignment at line {number}', 'configuration', path)
        try:
            tokens = shlex.split(raw, comments=True, posix=True)
        except ValueError as error:
            raise ValueError(f'ppo-harness.environment-format: quoting at line {number} in {path}') from None
        require(len(tokens) <= 1, 'environment-format', f'quote values containing spaces at line {number}', 'configuration', path)
        values[key] = tokens[0] if tokens else ''
    return values


def load_environment(path):
    if Path(path).is_file():
        for key, value in read_environment(path).items():
            os.environ.setdefault(key, value)


def tracking_destination(required=False):
    values = {key: os.environ.get(key, '') for key in KEYS}
    if required:
        require(all(values.values()), 'tracking-configuration', 'set WANDB_API_KEY, WANDB_ENTITY and WANDB_PROJECT', 'tracking')
    return dict(entity=values['WANDB_ENTITY'], project=values['WANDB_PROJECT'])
