"""Light numerical views over native-owned candidate and port domains.

No geometry, candidates, masks or feature values are constructed here. Only
selected conditional branches materialize tensor views; transport retains the
flat arrays and small immutable descriptors instead of a Python candidate tree.
"""
from collections.abc import Mapping, Sequence

import numpy as np
import torch

KINDS = ('place', 'insert_band', 'delete_band')


class RootChoices(Sequence):
    def __init__(self, rows):
        self.rows = rows

    def __len__(self):
        return len(self.rows)

    def __getitem__(self, index):
        if isinstance(index,slice):return [self[i] for i in range(*index.indices(len(self)))]
        row = self.rows[index]
        kind = KINDS[int(row[0])]
        return dict(kind=kind,rotation=int(row[2])) if kind=='place' else dict(kind=kind)

    def kinds(self):
        return [KINDS[index] for index in self.rows[:,0].tolist()]


class PortFeatures(Mapping):
    def __init__(self, ranges, bank):
        self.ranges, self.bank = ranges, bank
        self.cache = {}

    def __len__(self):
        return len(self.ranges)

    def __iter__(self):
        return (int(rotation) for rotation in self.ranges)

    def __getitem__(self, rotation):
        rotation = int(rotation)
        if rotation not in self.cache:
            start,end = self.ranges[str(rotation)]
            self.cache[rotation] = self.bank[start:end]
        return self.cache[rotation]

    def __reduce__(self):
        return type(self),(self.ranges,self.bank)


class PortGroups(Sequence):
    def __init__(self, metadata, bank):
        self.metadata, self.bank = metadata, bank
        self.cache = {}

    def __len__(self):
        return len(self.metadata)

    def __getitem__(self, owner):
        if isinstance(owner,slice):return [self[i] for i in range(*owner.indices(len(self)))]
        owner = int(owner)
        if owner not in self.cache:
            self.cache[owner] = [dict(group,features=PortFeatures(group['features'],self.bank))
                for group in self.metadata[owner]]
        return self.cache[owner]

    def __reduce__(self):
        return type(self),(self.metadata,self.bank)


def prepare(value):
    """Adopt owned NumPy arrays as Torch views without rebuilding their contents."""
    result = {key:torch.from_numpy(item) if isinstance(item,np.ndarray) else item for key,item in value.items()}
    result['edge_index'] = result.pop('edge_index_rows').T.contiguous()
    roots = result.pop('root_meta')
    result['actions'] = RootChoices(roots)
    result['root_owners'] = roots[:,1]
    result['port_groups'] = PortGroups(result.pop('port_metadata')['groups'],result.pop('port_features'))
    return result
