import fs from 'node:fs';
import path from 'node:path';
import { buildDedicated } from './dedicated.mjs';
try {
  const required=['recipes','facilities','items','transports','components','fixture','witness-output','detail-output'],args=process.argv.slice(2),o={};
  for(let i=0;i<args.length;i+=2){const k=args[i].slice(2);if(!args[i].startsWith('--')||!required.includes(k)||o[k]||!args[i+1])throw new Error(`Invalid argument ${args[i]}`);o[k]=args[i+1];}
  for(const k of required)if(!o[k])throw new Error(`Missing --${k}`);
  const outputs=['witness-output','detail-output'].map(k=>path.resolve(o[k]));
  if(new Set(outputs).size!==2||outputs.some(p=>fs.existsSync(p)))throw new Error('Expected distinct new output paths');
  const catalogs=Object.fromEntries(['recipes','facilities','items','transports','components'].map(k=>[k,JSON.parse(fs.readFileSync(o[k]))]));
  const spec=JSON.parse(fs.readFileSync(o.fixture)),start=performance.now(),result=buildDedicated(catalogs,spec);result.construction_ms=performance.now()-start;
  fs.writeFileSync(outputs[0],JSON.stringify(result.witness,null,2)+'\n',{flag:'wx'});fs.writeFileSync(outputs[1],JSON.stringify(result,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({status:'dedicated-fixture-generated-not-yet-validated',facilities:result.witness.placements.length,internal_lanes:result.bindings.length,bridges:result.witness.logistics_components.length,construction_ms:result.construction_ms}));
}catch(e){console.error(JSON.stringify({code:'dedicated-fixture-generation-failed',severity:'error',stage:'bound-interconnect',message:e.message}));process.exitCode=1;}
