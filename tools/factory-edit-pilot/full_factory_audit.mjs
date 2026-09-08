// Independent original-graph coverage, score and retained-state audit of the Rust boundary.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { args, read, write, hash, Probe, sourcePaths, catalogKeys } from '../../docs/benchmarks/layout-rl/io.mjs';
import { prepareRoot, checkRoot } from '../../docs/benchmarks/layout-rl/coverage.mjs';
import { transform } from '../../docs/benchmarks/layout-rl/geometry.mjs';
import { project } from '../../docs/benchmarks/constructive-factory-closure/model.mjs';
import { density } from '../../docs/benchmarks/layout-production/metrics.mjs';

const xy = p => `${p.x},${p.y}`;
const direction = (a,b) => [b.x-a.x,b.y-a.y];
const outward = {north:[0,-1],east:[1,0],south:[0,1],west:[-1,0]};
export function measure(w, catalogs) {
  const d = density(w,catalogs), ports = project(w,catalogs); let turns=0;
  for (const n of w.transport_networks) {
    const ins=new Map(),outs=new Map();
    const put=(m,p,v)=>{const k=xy(p),list=m.get(k)??[];list.push(v);m.set(k,list);};
    for(const s of n.segments){const v=direction(s.from,s.to);put(outs,s.from,v);put(ins,s.to,v);}
    for(const t of n.terminals){
      const source=t.direction==='output';let v;
      if(t.endpoint.kind==='facility'){
        const p=ports[t.endpoint.instance].find(p=>p.id===t.endpoint.port).position;
        v=source?direction(p,t.position):direction(t.position,p);
      }else{v=outward[t.endpoint.side].map(x=>source?-x:x);}
      put(source?ins:outs,t.position,v);
    }
    for(const p of n.cells)for(const a of ins.get(xy(p))??[])for(const b of outs.get(xy(p))??[])
      if(a[0]*b[0]+a[1]*b[1]===0)turns++;
  }
  return {definition:'used-area-layer-tiles-all-terminal-arms-v1',score:[d.area,d.transport_layer_cells,turns],
    width:d.bounds.width,height:d.bounds.height,facility_xy_cells:d.facility_xy_cells,
    transport_xy_cells:d.belt_xy_cells+d.pipe_xy_cells-d.belt_pipe_overlap_xy,
    occupied_xy_cells:d.occupied_xy_cells,empty_xy_cells:d.empty_xy_cells};
}

async function main(options){
  const inputPath=path.resolve(options.inputs),inputs=read(inputPath),matrix=read(options.matrix);
  const dataset={roots:inputs.rows.map(r=>({id:r.id,dialect:r.dialect,request:read(r.original),witness:read(r.initial)}))};
  const paths=read(inputs.catalogs),catalogs=Object.fromEntries(catalogKeys.map(k=>[k,read(paths[k])]));
  const probe=new Probe({...paths,validator:path.resolve(options.validator)}), rows=[];
  const sources=new Set([inputPath,inputs.catalogs,...inputs.rows.flatMap(r=>[r.original,r.initial,r.request]),path.resolve(options.matrix),path.resolve(options.validator),...Object.values(paths),...sourcePaths(fileURLToPath(import.meta.url))]);
  const check=async(root,w)=>{
    const validation=await probe.check(w);let coverage;
    try{coverage=checkRoot(root,prepareRoot(root,catalogs),w,validation);}
    catch(e){coverage={factory_validated:false,diagnostics:[{code:e.code??'coverage-error',message:e.message}]};}
    return {validation,coverage};
  };
  try{
    assert.equal(matrix.rows.length,inputs.rows.length);
    for(const row of matrix.rows){
      const root=dataset.roots.find(r=>r.id===row.id);assert(root,`Unknown original root: ${row.id}`);
      sources.add(row.best);const w=read(row.best),result=await check(root,w);
      assert(result.coverage.factory_validated,JSON.stringify(result));
      assert.deepEqual(w,transform(root.witness));
      assert.deepEqual(measure(w,catalogs),row.metrics);
      rows.push({id:row.id,best_sha256:hash(row.best),factory_validated:true,metrics:row.metrics});
    }
    const replay=path.resolve(options.replay),report=read(path.join(replay,'report.json'));
    const root=dataset.roots.find(r=>r.id===options['replay-root']);assert(root,'Unknown replay root');
    const source=report.proposal_source.path;sources.add(source);assert.equal(hash(source),report.proposal_source.sha256);
    const proposals=fs.readFileSync(source,'utf8').trimEnd().split('\n');
    const trace=fs.readFileSync(path.join(replay,'trace.jsonl'),'utf8').trimEnd().split('\n').map(JSON.parse);
    assert.equal(trace.length,proposals.length);
    let current=transform(root.witness),best=current;
    for(const [index,row]of trace.entries()){
      let w;try{w=JSON.parse(proposals[index]);}catch{assert.equal(row.status,'rejected');continue;}
      const result=await check(root,w);
      assert.equal(result.coverage.factory_validated,row.candidate_validated,`proposal ${index+1}`);
      if(row.candidate_validated){
        const metrics=measure(w,catalogs);assert.deepEqual(metrics,row.candidate);
        if(!['late','budget_exhausted'].includes(row.status)){
          current=transform(w);
          const a=metrics.score,b=measure(best,catalogs).score;
          const first=a.findIndex((v,i)=>v!==b[i]);if(first>=0&&a[first]<b[first])best=current;
        }
      }
      assert.deepEqual(measure(current,catalogs),row.current);
      assert.deepEqual(measure(best,catalogs),row.best);
    }
    for(const [name,w,metrics]of [['initial',transform(root.witness),report.initial_metrics],['current',current,report.final.current_metrics],['best',best,report.final.best_metrics]]){
      const p=path.join(replay,name+'.json');sources.add(p);assert.deepEqual(read(p),w);
      const result=await check(root,w);assert(result.coverage.factory_validated);
      assert.deepEqual(measure(w,catalogs),metrics);
    }
    sources.add(path.join(replay,'report.json'));sources.add(path.join(replay,'trace.jsonl'));
    write(path.resolve(options.output),{schema_version:1,status:'passed',rows,replay:{root:root.id,proposals:trace.length,
      best:measure(best,catalogs),factory_validated:true},provenance:[...sources].map(p=>({path:p,sha256:hash(p)}))});
    console.log(JSON.stringify({status:'passed',roots:rows.length,replay_proposals:trace.length,best:measure(best,catalogs).score}));
  }finally{probe.close();}
}

if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  main(args(['inputs','matrix','validator','replay','replay-root','output'])).catch(e=>{
    console.error(JSON.stringify({code:'factory.audit-failed',severity:'error',stage:'factory-audit',message:e.message}));process.exitCode=1;
  });
}
