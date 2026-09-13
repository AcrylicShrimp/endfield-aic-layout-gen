"""Bind a small native audit manifest from actual complete-corpus episodes."""
import argparse
from collections import defaultdict
from copy import deepcopy
import json
from pathlib import Path
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from blueprint_ppo_store import checked, load, read, sha
from blueprint_ppo_update import digest
from declarative_success_features import segment_row
from declarative_success_native_audit import outward, physical, reference, require, write


def prepare(corpus_path,output,preferred=()):
    began=time.monotonic();output=Path(output).resolve();manifest=read(corpus_path);root=Path(corpus_path).resolve().parent
    require(manifest['schema']=='declarative-success-arrivals-v1' and manifest['complete'],'corpus','complete actual-arrival corpus required')
    require(not output.exists(),'output','fresh case directory required');output.mkdir(parents=True)
    child=Path(manifest['child']['directory']);require(sha(child/'launch.json')==manifest['child']['launch_sha256'],'binding','child launch changed')
    config=read(child/'launch.json')['config'];groups=defaultdict(list);candidates=[];known=set()
    for boundary in manifest['boundaries']:
        require(time.monotonic()-began<120,'case-budget','120 second case preparation cap')
        row=load(checked(boundary['reference'],root))
        for attempt in row['attempts']:
            entry=attempt['entry'];key=(entry['stream'],entry['episode']);prefix=groups[key]
            known.add(entry['id'])
            if entry['retention_features_v1']['has_place'] and len(entry['labels'])<=8 and len(prefix)<=16:
                candidates.append(dict(entry=entry,key=attempt['key'],prefix=list(prefix)))
            prefix.extend(label['reference'] for label in entry['labels'])
    # Short complete prefixes minimize native work; no native trial-and-error
    # search or score-dependent parameter sweep is performed here.
    candidates.sort(key=lambda row:(len(row['prefix']),len(row['entry']['labels']),row['key']))
    compactions=[row for row in candidates if len(row['entry']['labels'])==1 and row['entry']['retention_features_v1']['exterior_release']>0]
    ordered=compactions[:1]+sorted(candidates,key=lambda row:(row['entry']['retention_features_v1']['exterior_release']>0,
        len(row['prefix']),len(row['entry']['labels']),row['key']))
    candidates=[];seen=set()
    for row in ordered:
        if tuple(row['key']) not in seen:candidates.append(row);seen.add(tuple(row['key']))
    cache={};inspected=0
    def sample(ref):
        n,i=ref
        if n not in cache:
            require(len(cache)<32,'case-budget','at most 32 compact source decodes')
            source=manifest['union_sources'][str(n)]['reference'];cache[n]=load(checked(source,root))
        return cache[n]['samples'][i]
    selected=[];have_compaction=False;outward_count=0
    for candidate in candidates[:64]:
        require(time.monotonic()-began<120,'case-budget','120 second case preparation cap')
        entry=candidate['entry'];samples=[sample(label['reference']) for label in entry['labels']];inspected+=1
        row=segment_row(samples,entry['id'])
        role=None
        if not have_compaction and len(samples)==1 and entry['retention_features_v1']['exterior_release']>0:
            role='non_loop_compaction';have_compaction=True
        elif outward_count<2 and outward(row):
            role='outward_success';outward_count+=1
        if role is not None:selected.append((candidate,samples,role))
        if have_compaction and outward_count==2:break
    cases=[]
    for candidate,samples,role in selected:
        entry=candidate['entry'];name=f'{role}-{entry["id"][:12]}';directory=output/name
        metadata=entry['input'];bank=Path(metadata.get('input_directory',config['training_input']['directory']))
        record=read(checked(metadata['reference'],bank))
        if metadata.get('input_directory'):
            value={k:record[k] for k in ('request','original')}
        else:value=read(checked(record['files']['input'],bank))
        require(digest(value)==metadata['input_sha256'],'input','actual request/start payload changed')
        write(directory/'input.json',value);write(directory/'expected-start.json',physical(samples[0]['observation']))
        write(directory/'expected-final.json',samples[-1]['validated_witness'])
        first_place=next(s for s in samples if s['action']['kind']=='place')
        instance=first_place['action']['instance'];observation=samples[0]['observation']
        owner=next(p for p in observation['facilities'] if p['instance']==instance)
        groups=first_place['input']['port_groups'][first_place['input']['root_owners'][first_place['decision']['root']]]
        ports=[dict(node=g['node'],port=observation['port_assignments'][g['node']]) for g in groups]
        original=dict(kind='place',instance=instance,x=owner['x'],y=owner['y'],rotation=owner['rotation'],ports=ports)
        step=min(n for n in config['actions']['translation_steps'] if n>0);band=min(config['actions']['band_counts'])
        moved=dict(original,x=original['x']+step)
        others=[p for p in observation['facilities'] if p['instance']!=instance]
        axes=[axis for axis,dimension in (('x','width'),('y','height')) if all(p[axis]+p[dimension]<=owner[axis] for p in others)]
        axis=axes[0] if axes else 'x';at=owner[axis] if axes else 0
        cases.append(dict(id=name,role=role,input=reference(directory/'input.json'),
            prefix=[sample(ref)['action'] for ref in candidate['prefix']],
            expected_start=reference(directory/'expected-start.json'),expected_final=reference(directory/'expected-final.json'),
            actions=[s['action'] for s in samples],
            variants=[dict(id='pose-out-and-back',family='pose_reversal',prepend=[moved,original]),
                dict(id='insert-then-pose-contract',family='band_pose_contraction',prepend=[dict(kind='insert_band',axis=axis,at=at,count=band),original])],
            provenance=dict(attempt_key=candidate['key'],entry_id=entry['id'],label_references=[l['reference'] for l in entry['labels']],
                prefix_references=candidate['prefix'],training_input=metadata,
                prefix_scope='Recorded changed-action prefix; its entire physical start must match before any variant runs.')))
    for prefix in preferred:
        if any(case['provenance'].get('entry_id','').startswith(prefix) for case in cases):continue
        cases.append(dict(id=f'preferred-{prefix}',role='preferred_historical_case',
            unavailable='Complete native midpoint/prefix for this historical scoring-export ID is not present in the new complete interval.',
            provenance=dict(entry_prefix=prefix,matched_new_interval=any(identity.startswith(prefix) for identity in known))))
    require(len(cases)<=8,'case-budget','at most eight baseline cases')
    parent_launch=Path(manifest['parent']['directory'])/'launch.json'
    spec={k:config[k] for k in ('probe_spec','native')}
    result=dict(schema='declarative-success-native-cases-v1',spec=spec,limits_from_launch=reference(parent_launch),cases=cases,
        provenance=dict(corpus=reference(corpus_path),script=reference(__file__),decoded_sources=len(cache),inspected_candidates=inspected,
            seconds=time.monotonic()-began,selection='Shortest-prefix single-place compaction, then two outward segments, inspecting zero-exterior candidates first; at most 64 candidates and 32 source decodes.'))
    write(output/'manifest.json',result)
    print(json.dumps(dict(cases=len(cases),actual_cases=len(selected),**{k:result['provenance'][k] for k in ('decoded_sources','inspected_candidates','seconds')})))
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--corpus',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True);parser.add_argument('--preferred',action='append',default=[])
    args=parser.parse_args()
    try:prepare(args.corpus,args.output,args.preferred);return 0
    except (OSError,KeyError,ValueError) as error:
        print(json.dumps(dict(code='success-native-audit.case-input',severity='error',stage='case-preparation',message=str(error),entity=str(args.corpus))),file=sys.stderr)
        return 2


if __name__=='__main__':raise SystemExit(main())
