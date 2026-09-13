"""Linux CPU allocation and lightweight process-tree resource observations."""
import os
from pathlib import Path
import shutil
import time

from blueprint_replay_contract import check


def cpu_allocation(workers=8):
    allowed = sorted(os.sched_getaffinity(0))
    physical = {}
    for cpu in allowed:
        root = Path(f'/sys/devices/system/cpu/cpu{cpu}/topology')
        key = ((root / 'physical_package_id').read_text().strip(), (root / 'core_id').read_text().strip())
        physical.setdefault(key, cpu)
    selected = list(physical.values())
    check(len(selected) >= workers + 1, 'execution-gate', 'resources',
          f'need {workers} independent worker cores and a separate coordinator core', allowed)
    group = next(line.split('::', 1)[1] for line in Path('/proc/self/cgroup').read_text().splitlines() if line.startswith('0::'))
    root = Path('/sys/fs/cgroup')
    current = root / group.lstrip('/')
    quotas = []
    while current.is_relative_to(root):
        path = current / 'cpu.max'
        if path.exists():
            quota, period = path.read_text().split()
            quotas.append(dict(path=str(path), quota=quota, period=int(period),
                               cores=None if quota == 'max' else int(quota) / int(period)))
        if current == root:
            break
        current = current.parent
    effective = min((q['cores'] for q in quotas if q['cores'] is not None), default=None)
    check(effective is None or effective >= workers + 1, 'execution-gate', 'resources',
          'CPU quota would serialize the registered worker allocation', quotas)
    return dict(allowed=allowed, workers=selected[:workers], coordinator=selected[workers:workers + 2],
                physical_core_count=len(physical), quotas=quotas, effective_quota=effective)


def process_tree(root_pid):
    processes = {}
    for path in Path('/proc').iterdir():
        if not path.name.isdigit():
            continue
        try:
            text = (path / 'stat').read_text()
            fields = text[text.rfind(')') + 2:].split()
            processes[int(path.name)] = dict(ppid=int(fields[1]), rss=int(fields[21]) * os.sysconf('SC_PAGE_SIZE'),
                cpu_seconds=(int(fields[11]) + int(fields[12])) / os.sysconf('SC_CLK_TCK'))
        except (OSError, ValueError, IndexError):
            continue
    selected = {root_pid}
    while True:
        children = {pid for pid, row in processes.items() if row['ppid'] in selected}
        if children <= selected:
            break
        selected |= children
    return {pid: processes[pid] for pid in selected if pid in processes}


class ResourceGuard:
    def __init__(self, output, seconds, max_rss, min_free, root_pid=None):
        self.output, self.began, self.seconds = Path(output), time.monotonic(), seconds
        self.max_rss, self.min_free = max_rss, min_free
        self.root_pid = root_pid or os.getpid()
        self.last_check, self.peak_rss, self.last = 0., 0, {}

    def __call__(self):
        now = time.monotonic()
        check(now - self.began < self.seconds, 'resource-exhausted', 'resources', 'wall-clock ceiling reached', self.output)
        if now - self.last_check < .5:
            return
        self.last_check = now
        processes = process_tree(self.root_pid)
        rss = sum(p['rss'] for p in processes.values())
        self.peak_rss = max(self.peak_rss, rss)
        free = shutil.disk_usage(self.output).free
        self.last = dict(processes=len(processes), rss=rss, peak_rss=self.peak_rss, free_bytes=free,
                         elapsed_seconds=now - self.began)
        check(rss < self.max_rss, 'resource-exhausted', 'resources', 'process-tree RSS ceiling reached', self.last)
        check(free >= self.min_free, 'resource-exhausted', 'resources', 'minimum disk reserve reached', self.last)
