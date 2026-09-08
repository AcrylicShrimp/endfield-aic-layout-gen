// Independent original-factory and per-axis audit after the constructive channel cutover.
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args, read, write, hash, Probe, catalogKeys } from '../../docs/benchmarks/layout-rl/io.mjs';
import { prepareRoot, checkRoot } from '../../docs/benchmarks/layout-rl/coverage.mjs';
import { measure } from './full_factory_audit.mjs';

function bridges(w, catalogs) {
  let sameItem = 0;
  const xy = p => `${p.x},${p.y}`, eq = (a,b) => BigInt(a.numerator)*BigInt(b.denominator)===BigInt(b.numerator)*BigInt(a.denominator);
  for (const bridge of w.logistics_components.filter(c=>c.kind==='bridge')) {
    const def=catalogs.components.components.find(c=>c.id===bridge.component), axes=new Map(), owners=new Set();
    assert(def&&def.transport===bridge.transport&&def.kind==='bridge');
    for(const n of w.transport_networks.filter(n=>n.component_ids.includes(bridge.id))){
      owners.add(n.id);assert.equal(n.transport,bridge.transport);
      assert(!n.terminals.some(t=>xy(t.position)===xy(bridge.position)));
      const incoming=n.segments.filter(s=>xy(s.to)===xy(bridge.position)),outgoing=n.segments.filter(s=>xy(s.from)===xy(bridge.position));
      assert.equal(incoming.length,outgoing.length);
      for(const a of incoming){
        const dx=a.to.x-a.from.x,dy=a.to.y-a.from.y,axis=dx===0?'vertical':'horizontal';
        assert.equal(Math.abs(dx)+Math.abs(dy),1);assert(!axes.has(axis));
        const b=outgoing.filter(s=>s.to.x-s.from.x===dx&&s.to.y-s.from.y===dy);assert.equal(b.length,1);assert(eq(a.rate,b[0].rate));
        assert(BigInt(a.rate.numerator)*BigInt(def.capacity.duration_ms)<=BigInt(def.capacity.quantity)*1000n*BigInt(a.rate.denominator));
        axes.set(axis,n.item);
      }
    }
    assert.equal(axes.size,2);assert(owners.size===1||owners.size===2);
    if(axes.get('horizontal')===axes.get('vertical')){assert.equal(owners.size,1);sameItem++;}
  }
  return {total:w.logistics_components.filter(c=>c.kind==='bridge').length,same_item:sameItem};
}

async function main(o){
  const admission=read(o.admission), baseline=read(o.baseline), study=read(path.join(o['legacy-study'],'protocol.json')),
    legacy=read(path.join(o['legacy-study'],'report.json')), roots=read(path.join(path.dirname(o.admission),'roots.json'));
  for(const source of admission.provenance)assert.equal(hash(source.path),source.sha256,source.path);
  const paths=Object.fromEntries(catalogKeys.map(k=>[k,roots.options[k]])), catalogs=Object.fromEntries(catalogKeys.map(k=>[k,read(paths[k])])),
    probe=new Probe({...paths,validator:path.resolve(o.validator)}), rows=[], preserved=[];
  try{
    assert.equal(admission.rows.length,baseline.rows.length);
    for(const row of admission.rows){
      const old=baseline.rows.find(r=>r.id===row.id);assert(old);
      for(const filename of ['source-plan.json','wiring.json'])assert.deepEqual(read(path.join(row.directory,filename)),read(path.join(old.directory,filename)));
      const record={id:row.id,previous_status:old.status,status:row.status,required:row.facilities,
        previous_covered:read(path.join(old.directory,'construction.json')).final_node.member_instances.length};
      if(row.status==='accepted'){
        const root=roots.roots.find(r=>r.id===row.id);assert(root);
        const validation=await probe.check(root.witness),coverage=checkRoot(root,prepareRoot(root,catalogs),root.witness,validation);
        assert(coverage.factory_validated,JSON.stringify(coverage));
        record.metrics=measure(root.witness,catalogs);record.bridges=bridges(root.witness,catalogs);record.factory_validated=true;
      }
      rows.push(record);
    }
    for(const c of study.cases){
      const files={'request.json':c.request,'initial.json':c.initial,'original-request.json':c.original,'reference.json':path.join(path.dirname(c.request),'reference.json')};
      for(const [name,digest] of Object.entries(c.files_sha256??{}))assert.equal(hash(files[name]),digest);
      const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},
        validation=await probe.check(root.witness),coverage=checkRoot(root,prepareRoot(root,catalogs),root.witness,validation);
      assert(coverage.factory_validated,JSON.stringify(coverage));
      const metrics=measure(root.witness,catalogs);assert.deepEqual(metrics,legacy.rows.find(r=>r.case===c.id).initial_metrics);
      preserved.push({id:c.id,initial_sha256:hash(c.initial),factory_validated:true,metrics,bridges:bridges(root.witness,catalogs)});
    }
  }finally{probe.close();}
  const sources=[o.admission,o.baseline,o.validator,path.join(o['legacy-study'],'protocol.json'),path.join(o['legacy-study'],'report.json'),fileURLToPath(import.meta.url),...Object.values(paths)];
  write(o.output,{schema_version:1,status:'passed',rows,preserved,sources:sources.map(p=>({path:path.resolve(p),sha256:hash(p)})),
    scope:'Every admitted original graph, independent bridge axes, and unchanged metrics for the twelve legacy roots; no policy transfer claim'});
  console.log(JSON.stringify({status:'passed',rows,preserved:preserved.length}));
}
main(args(['admission','baseline','legacy-study','validator','output'])).catch(e=>{console.error(e);process.exitCode=1;});
