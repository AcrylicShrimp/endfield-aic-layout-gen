"""One actual fresh input per global slot, shared across logical stream counts."""
from copy import deepcopy

from blueprint_training_input import FreshInputs
from blueprint_ppo_update import require


class CanonicalInputs:
    def __init__(self, config, library, module, blueprint_spec, editor):
        require(config['training_input']['canonical_streams']==1, 'declarative.batch-input: canonical stream count must be one')
        self.streams=config['training']['streams']
        selected=deepcopy(config);selected['training']['streams']=1
        self.store=FreshInputs(selected,library,module,blueprint_spec,editor=editor)
        self.root=self.store.root

    def mapped(self, metadata, stream, episode):
        require(0<=stream<self.streams and episode>=0, 'declarative.batch-input: logical identity')
        result=deepcopy(metadata);canonical=result['identity']
        require(canonical['stream']==0 and canonical['episode']==episode*self.streams+stream,
            'declarative.batch-input: actual canonical slot differs from logical episode')
        result['canonical_identity']=canonical
        result['identity']=dict(stream=stream,episode=episode,slot=canonical['slot'],first_seed=canonical['first_seed'])
        return result

    def acquire(self, stream, episode):
        payload,metadata=self.store.acquire(0,episode*self.streams+stream)
        return payload,self.mapped(metadata,stream,episode)

    def load(self, reference, stream, episode):
        payload,metadata=self.store.load(reference,0,episode*self.streams+stream)
        return payload,self.mapped(metadata,stream,episode)
