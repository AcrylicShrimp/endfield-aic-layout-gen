// Full ordered-description snapshots and timings for a frozen explicit case list.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gzipSync,gunzipSync} from 'node:zlib';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,sourcePaths} from '../../docs/benchmarks/layout-rl/io.mjs';
import {describe} from './full_factory_executor.mjs';
const o=args(['config','output'],['compare','case']),config=read(o.config),rows=[];
const catalogs=Object.fromEntries(Object.entries(read(config.catalogs)).map(([k,p])=>[k,read(p)]));
const stable=v=>Array.isArray(v)?v.map(stable):v&&typeof v==='object'?
  Object.fromEntries(Object.entries(v).filter(([k])=>k!=='seconds').map(([k,v])=>[k,stable(v)])):v;
const sources=[o.config,config.catalogs,...Object.values(read(config.catalogs)),...sourcePaths(fileURLToPath(import.meta.url))]
  .map(p=>({path:path.resolve(p),sha256:hash(p)}));
const cases=config.cases.filter(c=>!o.case||c.id===o.case);assert(cases.length);
fs.mkdirSync(o.output);
for(const c of cases){
  assert.equal(hash(c.witness),c.sha256,c.witness);
  const bytes=fs.readFileSync(c.witness),w=JSON.parse(c.witness.endsWith('.gz')?gunzipSync(bytes):bytes),before=JSON.stringify(w);
  const result=describe(w,catalogs,config.options??{});assert.equal(JSON.stringify(w),before);
  const filename=`${c.id}.json.gz`;fs.writeFileSync(path.join(o.output,filename),gzipSync(JSON.stringify(result)),{flag:'wx'});
  if(o.compare)assert.deepEqual(stable(result),stable(JSON.parse(gunzipSync(fs.readFileSync(path.join(o.compare,filename))))));
  rows.push({id:c.id,instances:w.placements.length,seconds:result.seconds,pose_seconds:result.poses.seconds,
    actions:result.actions.length,poses:result.poses.actions,description_sha256:hash(path.join(o.output,filename)),
    non_timing_equal:o.compare?true:null});
  sources.push({path:c.witness,sha256:c.sha256});console.log(JSON.stringify(rows.at(-1)));
}
for(const s of sources)assert.equal(hash(s.path),s.sha256,s.path);
write(path.join(o.output,'report.json'),{schema_version:1,status:'complete',rows,sources,
  comparison:o.compare?path.resolve(o.compare):null,scope:'All ordered description fields except nested seconds; immutable input witness'});
