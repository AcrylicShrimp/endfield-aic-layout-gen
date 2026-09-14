"""Self-contained metadata sharing must not retain dead inputs or alias arrays."""
from copy import deepcopy
import gc
import pickle
import unittest
from weakref import ref

import torch

from declarative_native_input import PortGroups,PortMetadata
from ppo_tensor_pickle import dumps


class MetadataOwnershipTests(unittest.TestCase):
    def test_independent_messages_share_only_equal_descriptors_and_release_them(self):
        groups=[[dict(node=1234567,graph_node=2,ports=['a'],features={'0':(0,1)})]]
        # Old full-input archives reconstruct this same list-based constructor.
        source=PortGroups(groups,torch.arange(12).reshape(1,12))
        packed=dumps(dict(groups=source));del source
        left=pickle.loads(packed)['groups'];right=pickle.loads(packed)['groups']
        self.assertIs(left.domain,right.domain)
        self.assertEqual(left.metadata,groups)
        left.bank[0,0]=99
        self.assertEqual(int(right.bank[0,0]),0)
        changed=deepcopy(groups);changed[0][0]['ports']=['b']
        other=pickle.loads(dumps(dict(groups=PortGroups(PortMetadata(changed),right.bank))))['groups']
        self.assertIsNot(other.domain,right.domain)
        lifetime=ref(left.domain)
        del left,right;gc.collect()
        self.assertIsNone(lifetime())
        # Self-contained bytes remain readable after the old cache entry dies.
        again=pickle.loads(packed)['groups']
        self.assertEqual(again.metadata,groups)
        self.assertEqual(int(again[0][0]['features'][0][0,0]),0)

    def test_lazy_views_are_local_while_immutable_domain_is_shared(self):
        groups=[[dict(node=8765432,graph_node=2,ports=['a'],features={'0':(0,1)})]]
        packed=dumps(dict(groups=PortGroups(PortMetadata(groups),torch.zeros(1,12))))
        first=pickle.loads(packed)['groups'];second=pickle.loads(packed)['groups']
        first[0][0]['features'][0]
        self.assertEqual(second.cache,{})
        self.assertIs(first.domain,second.domain)


if __name__=='__main__':unittest.main()
