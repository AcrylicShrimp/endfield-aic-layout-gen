import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
const sha=b=>createHash('sha256').update(b).digest('hex');
try {
  const keys=['validator','witness','recipes','facilities','items','transports','components','work-directory','report-output'],o={},args=process.argv.slice(2);
  for(let i=0;i<args.length;i+=2){const k=args[i].slice(2);if(!args[i].startsWith('--')||!keys.includes(k)||o[k]||!args[i+1])throw new Error(`Invalid option ${args[i]}`);o[k]=path.resolve(args[i+1]);}
  for(const k of keys)if(!o[k])throw new Error(`Missing --${k}`);
  if(fs.existsSync(o['work-directory'])||fs.existsSync(o['report-output']))throw new Error('Audit work directory and output must be new');
  fs.mkdirSync(o['work-directory']);const original=JSON.parse(fs.readFileSync(o.witness));
  const omitComponent=kind=>w=>{const removed=w.logistics_components.filter(c=>c.kind===kind).map(c=>c.id);w.logistics_components=w.logistics_components.filter(c=>c.kind!==kind);for(const n of w.transport_networks)n.component_ids=n.component_ids.filter(id=>!removed.includes(id));};
  const cases=[
    ['original',()=>{},true,true],
    ...['bridge','splitter','converger'].filter(kind=>original.logistics_components.some(c=>c.kind===kind)).map(kind=>['without-'+kind,omitComponent(kind),false,false]),
    ['wrong-port',w=>{w.transport_networks[0].terminals[0].endpoint.port='input-belt-0';},false,false],
    ['unknown-item',w=>{w.transport_networks[0].item='unknown';},false,false],
    ['missing-whole-network',w=>{w.transport_networks.splice(w.transport_networks.findIndex(n=>!n.component_ids.length),1);},true,false],
    ['four-times-rate',w=>{for(const u of Object.values(w.utilizations))u.numerator*=4;for(const n of w.transport_networks){for(const s of n.segments)s.rate.numerator*=4;for(const t of n.terminals)t.rate.numerator*=4;}},false,false],
  ];
  const results=[];
  for(const[name,change,valid,complete]of cases){const w=structuredClone(original);change(w);const witness=path.join(o['work-directory'],name+'.json'),report=path.join(o['work-directory'],name+'-validation.json'),bytes=JSON.stringify(w);
    fs.writeFileSync(witness,bytes,{flag:'wx'});const start=performance.now();
    const r=spawnSync(o.validator,['--witness',witness,...['recipes','facilities','items','transports','components'].flatMap(k=>['--'+k,o[k]]),'--report-output',report],{encoding:'utf8',timeout:30000});
    if(r.error)throw r.error;if(!fs.existsSync(report))throw new Error(`${name}: ${r.stderr}`);
    const validation=JSON.parse(fs.readFileSync(report));
    if(r.status!==(valid?0:2)||validation.covered_geometry_validated!==valid||validation.recipe_coverage_complete!==complete)throw new Error(`Unexpected audit result ${name}: ${JSON.stringify(validation)}`);
    results.push({name,expected_valid:valid,expected_complete:complete,witness_sha256:sha(bytes),validation,process_wall_ms:performance.now()-start});
  }
  const result={schema_version:1,status:'negative-controls-passed',cases:results,validator_sha256:sha(fs.readFileSync(o.validator)),input_sha256:sha(fs.readFileSync(o.witness))};
  fs.writeFileSync(o['report-output'],JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({status:result.status,cases:results.length,report_output:o['report-output']}));
}catch(e){console.error(JSON.stringify({code:'bound-interconnect-audit-failed',stage:'validation-audit',message:e.message}));process.exitCode=1;}
