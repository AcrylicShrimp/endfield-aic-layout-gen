import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,digest,catalogKeys,Probe} from '../layout-rl/io.mjs';
import {drawing} from '../layout-rl/visualize.mjs';
import {prepareRoot,checkRoot} from '../layout-rl/coverage.mjs';
import {score} from '../layout-rl/score.mjs';

try{
  const o=args(['plan','run','output-directory']),planPath=path.resolve(o.plan),plan=read(planPath),resolve=p=>path.resolve(path.dirname(planPath),p);
  const paths=Object.fromEntries(catalogKeys.map(k=>[k,resolve(plan.catalogs[k])])),catalogs=Object.fromEntries(catalogKeys.map(k=>[k,read(paths[k])]));
  const report=read(path.join(o.run,'report.json'));
  if(report.status!=='complete'||report.config.groups!==2||report.config.horizon!==32)throw Error('Expected the predefined update-audit run');
  const selected=[{group:1,worker:0,step:1,label:'7 facilities · step 1 · route + compaction'},
    {group:1,worker:0,step:2,label:'7 facilities · step 2 · rejected relocation'},
    {group:1,worker:0,step:3,label:'7 facilities · step 3 · valid temporary deterioration'},
    {group:1,worker:0,step:4,label:'7 facilities · step 4 · improved geometry, zero credit'},
    {group:2,worker:1,step:9,label:'59 facilities · step 9 · shorter route, zero credit'}];
  const probe=new Probe({...paths,validator:resolve(plan.validator)},plan.validator_ms),cases=[],audits=[],capsules=new Map();
  try{
    for(const event of selected){
      const ref=report.training.groups[event.group-1].update_capsule;
      if(hash(ref.path)!==ref.sha256)throw Error('Changed trajectory capsule');
      if(!capsules.has(event.group))capsules.set(event.group,read(ref.path));
      const c=capsules.get(event.group),root=c.root,episodeRef=c.episodes[event.worker];
      if(hash(episodeRef.path)!==episodeRef.sha256)throw Error('Changed trajectory episode');
      for(const p of Object.values(paths))if(!c.sources.some(s=>s.path===p&&s.sha256===hash(p)))throw Error('Changed trajectory catalog');
      const history=c.rollouts.histories[event.worker],episode=read(episodeRef.path),transition=episode.trace[event.step-1];
      const witness=row=>{
        const s=c.rollouts.states[row.state_ref];
        const w={...root.witness,placements:s.facilities.map(({ports,rotations,...p})=>p),transport_networks:s.networks,logistics_components:s.logistics_components,utilizations:s.utilizations};
        if(digest(w)!==s.state)throw Error('Reconstructed trajectory geometry hash mismatch');
        return w;
      };
      const before=witness(history[event.step-1]),after=witness(history[event.step]);
      if(digest(before)!==transition.parent_sha256||digest(after)!==(transition.accepted?transition.witness_sha256:transition.parent_sha256))throw Error('Trajectory edge does not join the selected frames');
      const aliases=new Map(root.witness.placements.map((p,i)=>[p.instance,`F${String(i+1).padStart(2,'0')}`])),graph=prepareRoot(root,catalogs);
      for(const [side,w]of [['before',before],['after',after]]){
        const validation=await probe.check(w),coverage=checkRoot(root,graph,w,validation);
        if(!coverage.factory_validated)throw Error('Trajectory frame failed original full-factory validation');
        audits.push({...event,side,witness_sha256:digest(w),validation,coverage});
      }
      const b=drawing(before,aliases,catalogs),a=drawing(after,aliases,catalogs),action=transition.action;
      const focus=(action.instances??[]).map(id=>aliases.get(id));
      if(action.terminal){const t=before.transport_networks.flatMap(n=>n.terminals).find(t=>t.id===action.terminal);if(t?.endpoint.kind==='facility')focus.push(aliases.get(t.endpoint.instance));}
      const changed=before.transport_networks.flatMap((n,i)=>digest(n)!==digest(after.transport_networks[i])?[i]:[]);
      const ghosts=!transition.accepted&&action.kind==='relocate'?b.facilities.filter(p=>focus.includes(p.alias)).map(p=>({...p,x:p.x+action.dx,y:p.y+action.dy})):[];
      cases.push({...event,before:b,after:a,credit:c.credit.signals[event.worker][event.step-1],best:transition.best,
        status:transition.accepted?'accepted':transition.diagnostics?.[0]?.code??transition.status,
        action_label:`${action.kind}${focus.length?' '+focus.join(', '):''}${action.dx!==undefined?` (${action.dx}, ${action.dy})`:''}${action.compact_after?' + compact':''}`,
        focus,changed_networks:changed,rejected_destinations:ghosts});
      if(JSON.stringify(score(before,catalogs).tuple)!==JSON.stringify(transition.before)||JSON.stringify(score(after,catalogs).tuple)!==JSON.stringify(transition.score??transition.before))throw Error('Trajectory frame score mismatch');
    }
  }finally{probe.close();}
  const templatePath=fileURLToPath(new URL('./trajectory-view.html',import.meta.url));
  const fragment=fs.readFileSync(templatePath,'utf8').replace('__TRAJECTORY_DATA__',JSON.stringify({cases}).replaceAll('<','\\u003c'));
  if(Buffer.byteLength(fragment)>=1000000)throw Error('Trajectory visual exceeds 1 MB');
  const output=path.resolve(o['output-directory']);fs.mkdirSync(output);
  fs.writeFileSync(path.join(output,'trajectory-inspection.html'),fragment,{flag:'wx'});
  write(path.join(output,'report.json'),{schema_version:1,status:'complete',selection:selected,selection_scope:'Qualitative post-hoc case inspection, not an unbiased policy-quality estimate',
    attempt_overlays_are_not_accepted_geometry:true,audits,provenance:[planPath,path.resolve(o.run,'report.json'),templatePath,fileURLToPath(import.meta.url)].map(p=>({path:p,sha256:hash(p)}))});
  console.log(JSON.stringify({status:'complete',cases:cases.length,validated_frames:audits.length,bytes:Buffer.byteLength(fragment),output}));
}catch(e){console.error(JSON.stringify({code:'trajectory-visual-failed',severity:'error',stage:'trajectory-inspection',message:e.message,entity:null}));process.exitCode=1;}
