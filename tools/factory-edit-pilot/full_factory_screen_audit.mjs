// Validate retained policy outputs against original production graphs and independent metrics.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,Probe,sourcePaths} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {measure} from './full_factory_audit.mjs';

const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const compare=(a,b)=>{const i=a.findIndex((x,i)=>x!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
async function main(options){
  const inputs=read(options.inputs),directory=path.resolve(options.screen),report=read(path.join(directory,'screen.json'));
  const protocol=read(path.join(directory,'protocol.json'));
  assert.equal(report.status,'complete');assert.equal(hash(path.join(directory,'protocol.json')),report.protocol_sha256);
  assert.equal(hash(options.inputs),protocol.inputs_sha256);
  for(const [relative,expected]of Object.entries(protocol.source_sha256))assert.equal(hash(relative),expected,relative);
  const paths=Object.fromEntries(Object.entries(protocol.catalogs).map(([k,v])=>{assert.equal(hash(v.path),v.sha256);return[k,v.path];}));
  const catalogs=Object.fromEntries(Object.entries(paths).map(([k,p])=>[k,read(p)]));
  const roots=new Map(inputs.rows.map(r=>{
    assert.equal(hash(r.original),r.files_sha256['original-request.json']);
    const root={id:r.id,dialect:r.dialect,request:read(r.original),witness:read(r.initial)};
    return[r.id,{root,prepared:prepareRoot(root,catalogs)}];
  }));
  roots.set('closure-strong',roots.get('closure-original'));
  const probe=new Probe({...paths,validator:path.resolve(options.validator)}),rows=[],seen=new Set();
  const actors=protocol.depths.flatMap(d=>protocol.seeds.map(s=>`d${d}-s${s}`));
  const expected=[];
  for(const actor of actors){
    for(const phase of ['untrained','final'])for(const c of protocol.cases)for(const seed of protocol.eval_seeds)
      expected.push(`${actor}-${phase}--${c.id}--${seed}`);
    for(let g=0;g<protocol.groups;g++)for(let a=0;a<protocol.group_episodes;a++)expected.push(`${actor}-train-${g}-${a}`);
  }
  for(const method of ['uniform','geometry'])for(const c of protocol.cases)for(const seed of protocol.eval_seeds)
    expected.push(`${method}--${c.id}--${seed}`);
  assert.equal(report.rows.length,expected.length);
  try{
    for(const row of report.rows){
      assert(!seen.has(row.directory));seen.add(row.directory);
      const p=path.join(directory,row.directory),w=unpack(path.join(p,'best.json.gz')),episode=unpack(path.join(p,'episode.json.gz'));
      const {root,prepared}=roots.get(row.case),validation=await probe.check(w);
      const coverage=checkRoot(root,prepared,w,validation);
      assert(coverage.factory_validated,JSON.stringify({case:row.case,coverage}));
      assert.deepEqual(measure(w,catalogs),row.best_metrics);
      assert(compare(row.best_metrics.score,row.initial_metrics.score)<=0);
      assert.equal(episode.trace.length,row.selected);
      assert.equal(row.selected,protocol.horizon,'Incomplete episode must be reported separately');
      let best=row.initial_metrics.score,expanded=0,native=0;
      for(const [i,t]of episode.trace.entries()){
        assert.equal(t.selected,i+1);assert(t.route_expansions<=protocol.route_work);expanded+=t.route_expansions;
        if(t.acceptance)native++;
        assert(compare(t.best_score,best)<=0);best=t.best_score;
        assert(!t.diagnostics.some(d=>d.code==='factory.executor-error'));
      }
      assert.equal(expanded,row.route_expansions);assert.equal(native,row.native_proposals);
      assert.deepEqual(best,row.best_metrics.score);
      rows.push({directory:row.directory,factory_validated:true,score:row.best_metrics.score,
        best_gzip_sha256:hash(path.join(p,'best.json.gz')),episode_gzip_sha256:hash(path.join(p,'episode.json.gz'))});
    }
    assert.deepEqual([...seen].sort(),expected.sort());
    assert.equal(report.groups.length,actors.length*protocol.groups);
    const groupIds=new Set();
    for(const group of report.groups){
      const id=`${group.arm}:${group.group}`;assert(!groupIds.has(id));groupIds.add(id);
      assert(actors.includes(group.arm)&&group.group>=0&&group.group<protocol.groups);
      assert.equal(group.case,protocol.training_roots[group.group%protocol.training_roots.length]);
      const members=report.rows.filter(r=>r.phase==='training'&&r.arm===group.arm&&r.group===group.group);
      assert.equal(members.length,protocol.group_episodes);
      assert.deepEqual(members.map(r=>r.best_metrics.score),group.scores);
      const ranks=group.scores.map(s=>group.scores.reduce((sum,t)=>sum-compare(s,t),0)/(protocol.group_episodes-1));
      assert.deepEqual(ranks,group.ranks);assert(Number.isFinite(group.loss)&&Number.isFinite(group.gradient_norm));
    }
    write(path.resolve(options.output),{schema_version:1,status:'passed',episodes:rows.length,rows,
      scope:'Original graph coverage and independent JS metrics; geometry uses the same production Rust validator',
      provenance:[options.inputs,path.join(directory,'screen.json'),path.join(directory,'protocol.json'),path.resolve(options.validator),
        ...sourcePaths(fileURLToPath(import.meta.url))].map(p=>({path:p,sha256:hash(p)}))});
    console.log(JSON.stringify({status:'passed',episodes:rows.length}));
  }finally{probe.close();}
}
main(args(['inputs','screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
