import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,digest,catalogKeys,Probe} from '../layout-rl/io.mjs';
import {drawing} from '../layout-rl/visualize.mjs';
import {prepareRoot,checkRoot} from '../layout-rl/coverage.mjs';
import {density,routeDiagnostics} from '../layout-production/metrics.mjs';
import {score} from '../layout-rl/score.mjs';

const compare=(a,b)=>{for(let i=0;i<a.length;i++)if(a[i]!==b[i])return a[i]-b[i];return 0;};
const checked=ref=>{if(hash(ref.path)!==ref.sha256)throw Error(`Changed evidence: ${ref.path}`);return read(ref.path);};
try{
  const o=args(['plan','run','output-directory']),run=path.resolve(o.run),reportPath=path.join(run,'report.json'),manifestPath=path.join(run,'manifest.json');
  const report=read(reportPath),manifest=read(manifestPath);
  if(report.status!=='complete'||report.behavior_actions!==8192||report.evaluation_actions!==32768)throw Error('Expected the complete predefined independent-batch pilot');
  const planPath=path.resolve(o.plan),plan=read(planPath),resolve=p=>path.resolve(path.dirname(planPath),p);
  const paths=Object.fromEntries(catalogKeys.map(k=>[k,resolve(plan.catalogs[k])])),catalogs=Object.fromEntries(catalogKeys.map(k=>[k,read(paths[k])]));
  for(const p of Object.values(paths))if(!manifest.sources.some(s=>s.path===p&&s.sha256===hash(p)))throw Error('Changed frozen-pilot catalog');
  const roots=manifest.roots.map(checked),groups=report.groups.map(checked),cases=[],selection=[],audits=[];
  const probe=new Probe({...paths,validator:resolve(plan.validator)},plan.validator_ms);
  try{
    for(let r=0;r<2;r++){
      const root=roots[r],candidates=[];
      for(const group of groups.filter(g=>g.root_index===r))for(let index=0;index<4;index++){
        const prefix=group.arms.prefix.evaluation[index],zero=group.arms.zero.evaluation[index];
        candidates.push({group:group.batch_index,index,prefix,zero,delta:prefix.summary.best_score.tuple.map((v,i)=>v-zero.summary.best_score.tuple[i])});
      }
      candidates.sort((a,b)=>compare(a.delta,b.delta)||a.group-b.group||a.index-b.index);
      const best=candidates[0],worst=candidates.filter(c=>compare(c.delta,candidates.at(-1).delta)===0).sort((a,b)=>a.group-b.group||a.index-b.index)[0];
      for(const [kind,c]of [['largest observed benefit',best],['largest observed regression',worst]]){
        if(selection.some(s=>s.root_index===r&&s.batch===c.group&&s.evaluation===c.index))continue;
        const zero=checked(c.zero.artifact),prefix=checked(c.prefix.artifact),graph=prepareRoot(root,catalogs);
        const aliases=new Map(root.witness.placements.map((p,i)=>[p.instance,`F${String(i+1).padStart(2,'0')}`]));
        const event={root_index:r,batch:c.group,evaluation:c.index,selection:kind,delta:c.delta};
        for(const [arm,e]of [['zero',zero],['prefix',prefix]]){
          if(digest(e.best_witness)!==e.best_witness_sha256)throw Error('Changed prospective witness');
          const validation=await probe.check(e.best_witness),coverage=checkRoot(root,graph,e.best_witness,validation);
          if(!coverage.factory_validated)throw Error('Prospective visual failed original complete-factory coverage');
          const metrics=density(e.best_witness,catalogs);
          if(compare(score(e.best_witness,catalogs).tuple,e.best_score.tuple)||JSON.stringify(metrics)!==JSON.stringify(e.density))throw Error('Visual geometry does not reproduce recorded score/density');
          audits.push({...event,arm,witness_sha256:e.best_witness_sha256,validation,coverage,density:metrics,routes:routeDiagnostics(e.best_witness)});
        }
        const direction=compare(c.delta,[0,0,0]),outcome=direction<0?'prefix better':direction>0?'prefix worse':'quality tie';
        cases.push({label:`${root.witness.placements.length} facilities · ${outcome} · B${c.group+1}/E${c.index+1}`,
          untrained:{...drawing(zero.best_witness,aliases,catalogs),density:density(zero.best_witness,catalogs)},
          trained:{...drawing(prefix.best_witness,aliases,catalogs),density:density(prefix.best_witness,catalogs)},
          horizon:128,seed:`batch ${c.group+1} / evaluation ${c.index+1}`,
          family:`Matched prospective rollout · area / tiles / turns delta ${c.delta.join(' / ')} · extrema, not a typical-effect estimate`});
        selection.push(event);
      }
    }
  }finally{probe.close();}
  const templatePath=fileURLToPath(new URL('./visual.html',import.meta.url));
  const fragment=fs.readFileSync(templatePath,'utf8').replace('Validation factory','Matched evaluation case')
    .replaceAll('Untrained policy','Zero-signal control').replaceAll('Trained policy','One prefix update')
    .replace('__STREAM_VISUAL_DATA__',JSON.stringify({cases}).replaceAll('<','\\u003c'));
  if(Buffer.byteLength(fragment)>=1000000)throw Error('Frozen-pilot visual exceeds 1 MB');
  const output=path.resolve(o['output-directory']);fs.mkdirSync(output);
  fs.writeFileSync(path.join(output,'frozen-update-layouts.html'),fragment,{flag:'wx'});
  write(path.join(output,'report.json'),{schema_version:1,status:'complete',selection,audits,
    selection_scope:'Prefix versus zero matched-pair extrema per root; no representative or generalization claim',
    provenance:[reportPath,manifestPath,planPath,templatePath,fileURLToPath(import.meta.url)].map(p=>({path:p,sha256:hash(p)}))});
  console.log(JSON.stringify({status:'complete',cases:cases.length,validated_frames:audits.length,bytes:Buffer.byteLength(fragment),output}));
}catch(e){console.error(JSON.stringify({code:'frozen-batch-visual-failed',severity:'error',stage:'frozen-batch-inspection',message:e.message,entity:null}));process.exitCode=1;}
