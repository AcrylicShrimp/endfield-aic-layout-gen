"""Replay every selected boundary-teacher edit and retain its strongest partial path."""
import argparse
from collections import Counter
import gzip
import json
from pathlib import Path
import time

from full_factory_environment import FactoryRuntime, FactoryEpisode
from full_factory_selection import physical_key
from full_factory_screen import sha, write, save_gzip
from full_factory_trajectory import boundary, potential, shortlist


def run(args):
    began=time.perf_counter();p=json.loads((args.run/'protocol.json').read_text());r=json.loads((args.run/'trajectory.json').read_text())
    assert r['protocol_sha256']==sha(args.run/'protocol.json')
    for file,expected in p['source_sha256'].items():assert sha(file)==expected,file
    assert sha(p['case']['initial'])==p['initial_sha256'];assert sha(args.native_library)==p['native_sha256']
    args.output.mkdir();scratch=args.output/'candidate-input.json';runtime=FactoryRuntime(args.native_library,p['catalogs'])
    nodes={n['id']:n for n in r['nodes']};witnesses={};descriptions={};counts=Counter();expansions=0
    try:
        root=FactoryEpisode(runtime,p['case'],1,30,p['route_work']);witnesses[0]=root.state['current'];weight=boundary(p['side'],r['initial'])
        assert physical_key(witnesses[0])==nodes[0]['physical_sha256'];assert root.state['current_metrics']==r['initial']
        for ordinal,row in enumerate(r['trace']):
            assert row['selected']==ordinal+1;parent=row['parent'];w=witnesses[parent]
            if parent not in descriptions:descriptions[parent]=runtime.rpc('describe',w,options={})
            assert row['action'] in shortlist(w,descriptions[parent],weight)[0]
            write(scratch,w);env=FactoryEpisode(runtime,dict(p['case'],initial=str(scratch.resolve())),1,30,p['route_work'])
            observed=env.step(row['action']);record=row['result']
            for key in ('status','executor_status','route_expansions','diagnostics','unchanged','current_score','best_score'):
                assert observed[key]==record[key],(ordinal,key)
            counts[observed['status']]+=1;expansions+=observed['route_expansions']
            if row['child'] is not None:
                child=row['child'];witnesses[child]=env.state['current'];node=nodes[child]
                assert node['parent']==parent and node['action']==row['action'];assert node['metrics']==env.state['current_metrics']
                assert physical_key(witnesses[child])==node['physical_sha256']
                assert potential(witnesses[child],node['metrics'],weight)==node['potential']
            if 'revisited_node' in row:assert physical_key(env.state['current'])==nodes[row['revisited_node']]['physical_sha256']
        assert len(r['trace'])==r['selected'];assert dict(counts)==r['counts'];assert len(nodes)==len(witnesses)
        assert r['best']==min(nodes,key=lambda i:(nodes[i]['metrics']['score'],i))
        for depth in r['depths']:
            expected=[];queues=[]
            for parent in depth['parents']:
                actions,pools=shortlist(witnesses[parent],descriptions[parent],weight);queues.append((parent,actions))
                assert dict(parent=parent,offered=pools,shortlisted=len(actions)) in depth['pools']
            for choice in range(max((len(q)for _,q in queues),default=0)):
                expected.extend((parent,q[choice])for parent,q in queues if choice<len(q))
            actual=[(x['parent'],x['action'])for x in r['trace']if nodes[x['parent']]['depth']==depth['depth']]
            assert actual==expected[:len(actual)];assert len(actual)==depth['selected']
        progress=min(nodes,key=lambda i:(nodes[i]['potential'],i));retained=set(r['retained_nodes']+[progress])
        if r['depths']:retained.update(r['depths'][-1]['parents'])
        for node in list(retained):
            while nodes[node]['parent'] is not None:node=nodes[node]['parent'];retained.add(node)
        for node in retained:
            save_gzip(args.output/f'node-{node}.json.gz',witnesses[node])
            if node in r['retained_nodes']:
                assert witnesses[node]==json.loads(gzip.decompress((args.run/f'node-{node}.json.gz').read_bytes()))
        write(args.output/'replay.json',dict(status='passed',source_sha256=sha(Path(__file__)),protocol_sha256=sha(args.run/'protocol.json'),
              report_sha256=sha(args.run/'trajectory.json'),selected=r['selected'],nodes=len(nodes),route_expansions=expansions,
              progress=progress,retained_nodes=sorted(retained),seconds=time.perf_counter()-began,
              scope='All selected actions, shortlist order, native transitions and duplicate identities; retained partial paths reconstructed from exact action records'))
        scratch.unlink();print(json.dumps(dict(status='passed',selected=r['selected'],nodes=len(nodes),progress=progress,seconds=time.perf_counter()-began)))
    finally:runtime.close()


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('run','native-library','output'):p.add_argument('--'+name,type=Path,required=True)
    run(p.parse_args())
