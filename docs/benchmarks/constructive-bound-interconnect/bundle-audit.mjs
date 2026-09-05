import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { crossingBundle } from './bundle.mjs';
import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
import { capacity, dto } from '../constructive-interconnect/rates.mjs';
const sha=b=>createHash('sha256').update(b).digest('hex');

// The unconnected catalog anchor is required by witness v1. Its pending recipe rates stay explicit.
// It sits outside all transport lanes and does not participate in the component being validated.
export function boundaryWitness(catalogs,anchorSpec,transport,n,m) {
  const recipe=catalogs.recipes.recipes.find(r=>r.id===anchorSpec.recipe),definition=catalogs.facilities.facilities.find(f=>f.id===recipe?.facility);
  if(!definition?.allowed_rotations.includes(anchorSpec.rotation))throw new Error('Invalid catalog anchor');
  const shape=rotateFacility(definition,anchorSpec.rotation),x=shape.width+3,y=shape.height+3,width=x+m+1,height=y+n+1;
  const items=catalogs.items.items.filter(i=>i.transport===transport).slice(0,2),value=dto(capacity(catalogs.transports.transports.find(t=>t.kind===transport).capacity));
  if(items.length!==2)throw new Error('Need two catalog commodities per transport');
  const lanes=(length,prefix,item)=>Array.from({length},(_,i)=>({id:prefix+i,item,transport,rate:value}));
  const h=lanes(n,'H',items[0].id),v=lanes(m,'V',items[1].id),module=crossingBundle(h,v,[x,y],catalogs);
  const networks=[h,v].map((group,axis)=>{
    const cells=[],segments=[],terminals=[];
    for(const [i,l]of group.entries()){
      const from=axis?{x:x+i,y:0}:{x:0,y:y+i},to=axis?{x:x+i,y:height-1}:{x:width-1,y:y+i};
      const line=polyline([[from.x,from.y],[to.x,to.y]]);cells.push(...line);
      for(let j=1;j<line.length;j++)segments.push({from:line[j-1],to:line[j],rate:value});
      for(const [suffix,position,direction,side]of [['supply',from,'output',axis?'north':'west'],['demand',to,'input',axis?'south':'east']]){
        const id=l.id+':'+suffix;terminals.push({id,node:id,position,direction,endpoint:{kind:'external',node:id,side},rate:value});
      }
    }
    return{id:axis?'vertical':'horizontal',item:group[0].item,transport,requirement_ids:group.map(l=>l.id),cells,segments,terminals,component_ids:module.components.map(c=>c.id)};
  });
  return{schema_version:1,placements:[{instance:'catalog-anchor',recipe:recipe.id,facility:definition.id,x:1,y:1,width:shape.width,height:shape.height,rotation:anchorSpec.rotation}],
    utilizations:{'catalog-anchor':{numerator:1,denominator:1}},transport_networks:networks,logistics_components:module.components.map(({lanes,...c})=>c)};
}

if(process.argv[1]&&path.resolve(process.argv[1])===new URL(import.meta.url).pathname)try{
  const keys=['validator','recipes','facilities','items','transports','components','fixture','work-directory','report-output','max-lanes'],o={},args=process.argv.slice(2);
  for(let i=0;i<args.length;i+=2){const k=args[i].slice(2);if(!args[i].startsWith('--')||!keys.includes(k)||o[k]||!args[i+1])throw new Error(`Invalid option ${args[i]}`);o[k]=args[i+1];}
  for(const k of keys)if(!o[k])throw new Error(`Missing --${k}`);
  const maximum=Number(o['max-lanes']);if(!Number.isSafeInteger(maximum)||maximum<1||maximum>16)throw new Error('Diagnostic max-lanes must be 1..16');
  if(fs.existsSync(o['work-directory'])||fs.existsSync(o['report-output']))throw new Error('Expected new work directory and report');
  fs.mkdirSync(o['work-directory']);const catalogs=Object.fromEntries(['recipes','facilities','items','transports','components'].map(k=>[k,JSON.parse(fs.readFileSync(o[k]))]));
  const anchor=JSON.parse(fs.readFileSync(o.fixture)).nodes[0],cases=[];
  for(const transport of ['belt','pipe'])for(let n=1;n<=maximum;n++)for(let m=1;m<=maximum;m++){
    const witness=boundaryWitness(catalogs,anchor,transport,n,m),name=`${transport}-${n}x${m}`,file=path.join(o['work-directory'],name+'.json'),report=path.join(o['work-directory'],name+'-validation.json');
    fs.writeFileSync(file,JSON.stringify(witness),{flag:'wx'});
    const r=spawnSync(path.resolve(o.validator),['--witness',file,...['recipes','facilities','items','transports','components'].flatMap(k=>['--'+k,o[k]]),'--report-output',report],{encoding:'utf8',timeout:30000});
    if(r.error||r.status!==0)throw new Error(`${name} rejected: ${r.stderr||r.error}; report ${report}`);
    const validation=JSON.parse(fs.readFileSync(report));
    if(!validation.covered_geometry_validated||validation.recipe_coverage_complete||validation.recipe_terminals.some(t=>t.routed_rate.numerator!==0))throw new Error(`Incorrect primitive scope ${name}`);
    cases.push({transport,horizontal_lanes:n,vertical_lanes:m,bridge_count:witness.logistics_components.length,
      covered_geometry_validated:true,recipe_coverage_complete:false,pending_anchor_terminals:validation.recipe_terminals.length,elapsed_ms:validation.elapsed_ms,witness_sha256:sha(fs.readFileSync(file))});
  }
  const result={schema_version:1,status:'orthogonal-bundle-matrix-validated',scope:'component-boundary-geometry-not-factory',max_lanes:maximum,cases,
    proof_scope:'Cross two internally parallel lane bundles with different commodities across axes; no merging or lane-order permutation is synthesized.',
    validator_sha256:sha(fs.readFileSync(o.validator)),bundle_source_sha256:sha(fs.readFileSync(new URL('./bundle.mjs',import.meta.url))),
    inputs:Object.fromEntries(['recipes','facilities','items','transports','components','fixture'].map(k=>[k,{path:o[k],sha256:sha(fs.readFileSync(o[k]))}]))};
  fs.writeFileSync(o['report-output'],JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({status:result.status,cases:cases.length,largest_bridge_count:maximum*maximum,report_output:o['report-output']}));
}catch(e){console.error(JSON.stringify({code:'bundle-matrix-validation-failed',stage:'component-boundary-validation',severity:'error',message:e.message}));process.exitCode=1;}
