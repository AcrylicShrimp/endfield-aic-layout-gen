"""One-time export of authenticated retained inputs for the native collector gate.

Use the original source root's readers and library to validate its input contract;
never relabel an old generator bank as produced by the new native build.
"""
import argparse
import json
from pathlib import Path
import sys


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-root',type=Path,required=True)
    parser.add_argument('--launch',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--streams',type=int,default=128)
    args=parser.parse_args()
    assert not args.output.exists()
    sys.path.insert(0,str(args.source_root/'tools/factory-edit-pilot'))
    from blueprint_ppo_store import read,write,reference,sha
    from declarative_learning_environment import Runtime
    launch=read(args.launch)
    config=launch['config']
    assert 1<=args.streams<=config['training']['streams']
    runtime=Runtime(config,launch['source']['native_library'])
    args.output.mkdir(parents=True)
    episodes=[]
    for i in range(args.streams):
        record=runtime.inputs.root/f'episode-{i:08d}'/'episode.json'
        payload,metadata=runtime.inputs.load(reference(record,runtime.inputs.root),i,0)
        refs={}
        for name in ('request','original'):
            path=args.output/f'{i:04}-{name}.json'
            write(path,payload[name]);refs[name]=dict(path=path.name,sha256=sha(path))
        episodes.append(dict(id=f'canonical-slot-{metadata["identity"]["slot"]}',source=metadata,
            geometry_sha256=metadata['geometry_sha256'],**refs))
    result=dict(schema='aic-native-collection-input-v1',catalog_root=str(args.source_root.resolve()),catalogs=runtime.catalogs,
        episodes=episodes,reserved_geometry=sorted(runtime.reserved),options=config['actions'],limits=config['native'],
        workers=min(16,args.streams),chunk_actions=config['training']['chunk_actions'],gamma=config['ppo']['gamma'],
        gae_lambda=config['ppo']['gae_lambda'],advantage_normalization=config['ppo']['advantage_normalization'],
        minibatch_size=min(config['ppo']['minibatch_size'],args.streams*config['training']['chunk_actions']),max_observation_bytes=2*1024**3)
    write(args.output/'manifest.json',result)
    write(args.output/'source.json',dict(launch=str(args.launch.resolve()),launch_sha256=sha(args.launch),
        original_library=launch['source']['native_library'],original_native_sha256=launch['source']['native_sha256'],
        contract='Original source reader validated the retained actual-input contract; these are diagnostic exports, not newly generated inputs'))
    print(json.dumps(dict(streams=len(episodes),manifest=str(args.output/'manifest.json'))))


if __name__=='__main__':main()
