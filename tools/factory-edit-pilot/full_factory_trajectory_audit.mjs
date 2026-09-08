// Original-request and independent physical-metric audit of retained teacher states.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,Probe,sourcePaths} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {measure} from './full_factory_audit.mjs';
import {density} from '../../docs/benchmarks/layout-production/metrics.mjs';
async function main(o){
  const p=read(path.join(o.run,'protocol.json')),r=read(path.join(o.run,'trajectory.json')),replay=read(path.join(o.replay,'replay.json'));
  assert.equal(replay.status,'passed');assert.equal(hash(path.join(o.run,'trajectory.json')),replay.report_sha256);
  const c=p.case;assert.equal(hash(c.initial),p.initial_sha256);assert.equal(hash(c.original),c.files_sha256['original-request.json']);
  assert.equal(hash(c.request),c.files_sha256['request.json']);
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>[k,read(v)]));
  const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},prepared=prepareRoot(root,catalogs);
  const probe=new Probe({...paths,validator:path.resolve(o.validator)}),rows=[];
  try{
    for(const id of replay.retained_nodes){
      const file=path.join(o.replay,`node-${id}.json.gz`),w=JSON.parse(gunzipSync(fs.readFileSync(file))),n=r.nodes.find(n=>n.id===id);
      assert(checkRoot(root,prepared,w,await probe.check(w)).factory_validated);assert.deepEqual(measure(w,catalogs),n.metrics);
      rows.push({id,factory_validated:true,score:n.metrics.score,density:density(w,catalogs),sha256:hash(file)});
    }
    if(r.area_node!==null)assert(r.nodes[r.area_node].metrics.score[0]<r.initial.score[0]);
    write(o.output,{status:'passed',states:rows.length,rows,scope:'Original production graph, native geometry and independently measured score/density',
      provenance:[path.join(o.run,'protocol.json'),path.join(o.run,'trajectory.json'),path.join(o.replay,'replay.json'),o.validator,...Object.values(paths),...sourcePaths(fileURLToPath(import.meta.url))].map(file=>({path:file,sha256:hash(file)}))});
    console.log(JSON.stringify({status:'passed',states:rows.length}));
  }finally{probe.close();}
}
main(args(['run','replay','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
