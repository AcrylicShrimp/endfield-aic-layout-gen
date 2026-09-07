import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args,read,write,hash,digest,catalogKeys,Probe } from '../layout-rl/io.mjs';
import { drawing } from '../layout-rl/visualize.mjs';
import { prepareRoot,checkRoot } from '../layout-rl/coverage.mjs';
import { score,compare } from '../layout-rl/score.mjs';
import { density,routeDiagnostics } from '../layout-production/metrics.mjs';

try{
const options=args(['plan','report','roots','output-directory']);
const planPath=path.resolve(options.plan),plan=read(planPath),report=read(options.report),roots=read(options.roots);
if(report.status!=='complete')throw new Error('Visual comparison requires a completed run');
const resolve=p=>path.resolve(path.dirname(planPath),p),paths=Object.fromEntries(catalogKeys.map(k=>[k,resolve(plan.catalogs[k])]));
const catalogs=Object.fromEntries(catalogKeys.map(k=>[k,read(paths[k])]));
const recorded=read(path.join(path.dirname(path.resolve(options.report)),'worker-0/generation/provenance.json'));
for(const p of Object.values(paths)){if(recorded.find(s=>s.path===p)?.sha256!==hash(p))throw new Error(`Changed visual catalog: ${p}`);}
const horizon=Math.max(...report.config.evaluation_horizons),seed=report.config.evaluation_seeds[0];
const rows=report.evaluation.filter(e=>e.horizon===horizon&&e.seed===seed&&e.model==='trained'&&e.split==='validation');
const control=row=>report.evaluation.find(e=>e.root===row.root&&e.horizon===horizon&&e.seed===seed&&e.model==='initial');
const sizes=new Map(roots.map(r=>[r.id,r.witness.placements.length]));
const largest=[...rows].sort((a,b)=>sizes.get(b.root)-sizes.get(a.root)||a.root.localeCompare(b.root))[0];
const worst=[...rows].sort((a,b)=>b.summary.best_score.area/control(b).summary.best_score.area-a.summary.best_score.area/control(a).summary.best_score.area||a.root.localeCompare(b.root))[0];
const selection=[{row:largest,label:`Largest validation factory · ${sizes.get(largest.root)} facilities`},
  {row:worst,label:`Largest relative area regression · ${sizes.get(worst.root)} facilities`}];
const probe=new Probe({...paths,validator:resolve(plan.validator)},plan.validator_ms),cases=[],audits=[];
try{
  for(const {row,label}of selection){
    const root=roots.find(r=>r.id===row.root),source=report.generation.find(g=>g.root&&read(g.root.path).id===row.root)?.root;
    if(!source||hash(source.path)!==source.sha256||digest(read(source.path))!==digest(root))throw new Error('Changed original evaluation root');
    const graph=prepareRoot(root,catalogs),aliases=new Map(root.witness.placements.map((p,i)=>[p.instance,`F${String(i+1).padStart(2,'0')}`])),sides=[];
    for(const e of [control(row),row]){
      if(hash(e.artifact.path)!==e.artifact.sha256)throw new Error('Changed selected evaluation episode');
      const episode=read(e.artifact.path),w=episode.best_witness;
      if(digest(w)!==e.summary.best_witness_sha256||compare(score(w,catalogs).tuple,e.summary.best_score.tuple))throw new Error('Visual score/geometry mismatch');
      const validation=await probe.check(w),coverage=checkRoot(root,graph,w,validation);
      if(!coverage.factory_validated)throw new Error('Visual source failed original factory validation');
      const metrics=density(w,catalogs),routes=routeDiagnostics(w);
      audits.push({root:root.id,model:e.model,source:e.artifact,witness_sha256:digest(w),validation,coverage,density:metrics,routes});
      sides.push({...drawing(w,aliases,catalogs),density:metrics});
    }
    cases.push({root:root.id,label,family:root.family,horizon,seed,untrained:sides[0],trained:sides[1]});
  }
}finally{probe.close();}
const templatePath=fileURLToPath(new URL('./visual.html',import.meta.url));
const fragment=fs.readFileSync(templatePath,'utf8').replace('__STREAM_VISUAL_DATA__',JSON.stringify({cases}).replaceAll('<','\\u003c'));
if(Buffer.byteLength(fragment)>=1000000)throw new Error('Visual exceeds 1 MB');
const output=path.resolve(options['output-directory']);fs.mkdirSync(output);
fs.writeFileSync(path.join(output,'streaming-policy-comparison.html'),fragment,{flag:'wx'});
write(path.join(output,'report.json'),{schema_version:1,selection:'Largest facility count and largest trained/untrained area ratio; first evaluation seed, longest horizon',
  audits,provenance:[options.report,options.roots,planPath,templatePath,fileURLToPath(import.meta.url)].map(p=>({path:path.resolve(p),sha256:hash(p)}))});
console.log(JSON.stringify({status:'complete',cases:cases.length,independently_validated_witnesses:audits.length,output,bytes:Buffer.byteLength(fragment)}));
}catch(e){console.error(JSON.stringify({code:'stream-visual-failed',severity:'error',stage:'visual-audit',message:e.message,entity:null}));process.exitCode=1;}
