import fs from 'node:fs';
import { render } from './view.mjs';
try {
  const args=process.argv.slice(2),keys=['detail','validation','transports','output'],o={};
  for(let i=0;i<args.length;i+=2){const k=args[i].slice(2);if(!args[i].startsWith('--')||!keys.includes(k)||o[k]||!args[i+1])throw new Error(`Invalid option ${args[i]}`);o[k]=args[i+1];}
  for(const k of keys)if(!o[k])throw new Error(`Missing --${k}`);
  const read=k=>JSON.parse(fs.readFileSync(o[k]));
  fs.writeFileSync(o.output,render(read('detail'),read('validation'),{transports:read('transports')}),{flag:'wx'});
  console.log(JSON.stringify({status:'bound-interconnect-rendered',output:o.output}));
}catch(e){console.error(JSON.stringify({code:'bound-interconnect-render-failed',stage:'visualization',message:e.message}));process.exitCode=1;}
