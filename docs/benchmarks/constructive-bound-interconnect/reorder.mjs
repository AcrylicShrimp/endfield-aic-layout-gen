import fs from 'node:fs';
import { orderInputs } from './order-ports.mjs';
import { buildDedicated } from './dedicated.mjs';
try{
  const keys=['recipes','facilities','items','transports','components','fixture','report-output','witness-output','detail-output'],o={},args=process.argv.slice(2);
  for(let i=0;i<args.length;i+=2){const k=args[i].slice(2);if(!args[i].startsWith('--')||!keys.includes(k)||o[k]||!args[i+1])throw new Error(`Invalid option ${args[i]}`);o[k]=args[i+1];}
  for(const k of keys)if(!o[k])throw new Error(`Missing --${k}`);
  const outputs=['report-output','witness-output','detail-output'].map(k=>o[k]);if(new Set(outputs).size!==3||outputs.some(p=>fs.existsSync(p)))throw new Error('Expected distinct new outputs');
  const catalogs=Object.fromEntries(['recipes','facilities','items','transports','components'].map(k=>[k,JSON.parse(fs.readFileSync(o[k]))])),spec=JSON.parse(fs.readFileSync(o.fixture)),start=performance.now();
  const proposal=orderInputs(catalogs,spec),detail=buildDedicated(catalogs,proposal.spec);detail.case='source-ordered-ports';detail.construction_ms=performance.now()-start;
  fs.writeFileSync(o['report-output'],JSON.stringify(proposal,null,2)+'\n',{flag:'wx'});fs.writeFileSync(o['witness-output'],JSON.stringify(detail.witness,null,2)+'\n',{flag:'wx'});fs.writeFileSync(o['detail-output'],JSON.stringify(detail,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({status:'port-order-candidate-requires-validation',decisions:proposal.decisions.length,bridges:detail.witness.logistics_components.length,construction_ms:detail.construction_ms}));
}catch(e){console.error(JSON.stringify({code:'port-order-experiment-failed',stage:'port-order',severity:'error',message:e.message}));process.exitCode=1;}
