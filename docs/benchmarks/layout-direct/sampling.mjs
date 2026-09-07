import { createHash } from 'node:crypto';
import { digest } from '../layout-rl/io.mjs';

const fail=(code,message,entity=null)=>{throw Object.assign(new Error(message),{code:`stream-${code}`,severity:'warning',stage:'sampling',entity});};
export function validateSampling(p){
  if(p?.schema_version!==1||typeof p.partition_salt!=='string'||!p.partition_salt)throw new Error('Invalid sampling schema/partition salt');
  if(Object.keys(p.partition_buckets??{}).sort().join()!=='test,train,validation'||Object.values(p.partition_buckets).some(n=>!Number.isSafeInteger(n)||n<1))throw new Error('Invalid partition buckets');
  for(const [k,max]of [['max_source_nodes',4096],['max_depth',128],['max_facilities',256]])if(!Number.isSafeInteger(p[k])||p[k]<1||p[k]>max)throw new Error(`Invalid sampling guard ${k}`);
  if(!Array.isArray(p.utilizations)||!p.utilizations.length||p.utilizations.some(u=>!Number.isSafeInteger(u.numerator)||!Number.isSafeInteger(u.denominator)||u.numerator<1||u.denominator<u.numerator))throw new Error('Invalid sampled utilization fractions');
  if(!['all-catalog-inputs','catalog-external-items'].includes(p.procurement_pool)||!['item','occurrence'].includes(p.source_selection_scope))throw new Error('Invalid procurement pool/source scope');
  if(!Number.isFinite(p.procurement_probability)||p.procurement_probability<0||p.procurement_probability>1)throw new Error('Invalid declared procurement probability');
}
export function partitionFor(item,policy){
  const n=Object.values(policy.partition_buckets).reduce((a,b)=>a+b,0);
  let bucket=Number(BigInt('0x'+digest([policy.partition_salt,item]))%BigInt(n));
  for(const split of ['train','validation','test']){if(bucket<policy.partition_buckets[split])return split;bucket-=policy.partition_buckets[split];}
  throw new Error('Partition accounting failure');
}

class Random {
  constructor(seed,ordinal){this.key=JSON.stringify([seed,ordinal]);this.counter=0;}
  uint(){return createHash('sha256').update(`${this.key}:${this.counter++}`).digest().readUInt32LE(0);}
  index(n){if(!Number.isSafeInteger(n)||n<1||n>0xffffffff)throw new Error('Invalid random choice size');const ceiling=Math.floor(0x100000000/n)*n;let v;do{v=this.uint();}while(v>=ceiling);return v%n;}
  choice(xs){return xs[this.index(xs.length)];}
  float(){return this.uint()/0x100000000;}
}

// Every call derives a fresh request from catalogs and a replayable stream position.
// No geometry, constructor result, saved root or useful-action score is an input.
export function sampleRequest(catalogs,policy,{seed,ordinal,split='train'}){
  validateSampling(policy);
  if(typeof seed!=='string'||!seed||!Number.isSafeInteger(ordinal)||ordinal<0||!['train','validation','test'].includes(split))throw new Error('Invalid sample stream position');
  const recipes=catalogs.recipes.recipes,external=new Set(catalogs.recipes.external_items),producers=new Map();
  for(const recipe of recipes)for(const item of new Set(recipe.outputs.map(o=>o.item))){const xs=producers.get(item)??[];xs.push(recipe);producers.set(item,xs);}
  for(const xs of producers.values())xs.sort((a,b)=>a.id.localeCompare(b.id));
  const targets=[...producers.keys()].filter(item=>partitionFor(item,policy)===split).sort();
  if(!targets.length)fail('empty-partition','No runtime-catalog target belongs to this split',split);
  const rng=new Random(seed,ordinal),item=rng.choice(targets),sink=rng.choice(producers.get(item)),u=rng.choice(policy.utilizations);
  const quantity=sink.outputs.filter(o=>o.item===item).reduce((a,o)=>a+o.quantity,0)*u.numerator,duration_ms=sink.duration_ms*u.denominator;
  if(!Number.isSafeInteger(quantity)||!Number.isSafeInteger(duration_ms))fail('rate-guard','Requested target rate exceeds exact integer DTO range',item);
  const request={schema_version:1,target:{item,quantity,duration_ms},source_selections:[]},trace=[],selectedByItem=new Map([[item,sink]]);
  const visit=(commodity,path,ancestors,depth,forced=null)=>{
    if(trace.length>=policy.max_source_nodes||depth>policy.max_depth)fail('source-guard','Whole sampled source hierarchy exceeds declared guard',{seed,ordinal,item,path});
    const options=producers.get(commodity)??[];
    const known=policy.source_selection_scope==='item'&&selectedByItem.has(commodity);
    const mayProcure=policy.procurement_pool==='all-catalog-inputs'||external.has(commodity);
    const procure=!forced&&(known?selectedByItem.get(commodity)===null:mayProcure&&(!options.length||rng.float()<policy.procurement_probability));
    if(!forced&&!options.length&&!procure)fail('unsupported-raw-input','No catalog producer or declared external procurement source',{item:commodity,path});
    const selected=procure?null:forced??(known?selectedByItem.get(commodity):rng.choice(options)),source=selected?{kind:'recipe',recipe:selected.id}:{kind:'external-input'};
    if(policy.source_selection_scope==='item')selectedByItem.set(commodity,selected);
    request.source_selections.push({path,source});
    const cycle_to=selected?ancestors.get(selected.id):null;
    trace.push({path,item:commodity,source,cycle_to:cycle_to??null,declared_external:procure,alternatives:options.map(r=>r.id)});
    if(!selected||cycle_to)return;
    const next=new Map(ancestors);next.set(selected.id,path);
    for(const input of selected.inputs)visit(input.item,`${path}/recipe:${selected.id}/input:${input.item}`,next,depth+1);
  };
  visit(item,'/target',new Map(),0,sink);
  const structure_sha256=digest({target:item,source_selections:request.source_selections});
  return {schema_version:1,seed,ordinal,split,request,trace,structure_sha256,request_sha256:digest(request),
    selected_recipes:[...new Set(trace.filter(t=>t.source.kind==='recipe').map(t=>t.source.recipe))].sort(),
    source_cycles:trace.filter(t=>t.cycle_to).length,source_nodes:trace.length};
}
