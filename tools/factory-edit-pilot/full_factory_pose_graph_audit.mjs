// Independent incoming-direction traversal for the physical facility reachability graph.
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash} from '../../docs/benchmarks/layout-rl/io.mjs';
import {productionEdges} from './full_factory_poses.mjs';
const xy=p=>`${p.x},${p.y}`;
export function directedFacilityPairs(w){
  const indices=new Map(w.placements.map((p,i)=>[p.instance,i])),result=new Set();
  for(const n of w.transport_networks){
    const bridges=new Set(w.logistics_components.filter(c=>c.kind==='bridge'&&c.transport===n.transport).map(c=>xy(c.position))),out=new Map(),sinks=new Map();
    for(const s of n.segments){const p=xy(s.from),a=out.get(p)??[];a.push(s);out.set(p,a);}
    for(const t of n.terminals)if(t.direction==='input'&&t.endpoint.kind==='facility'){
      const p=xy(t.position),a=sinks.get(p)??[];a.push(indices.get(t.endpoint.instance));sinks.set(p,a);
    }
    for(const t of n.terminals)if(t.direction==='output'&&t.endpoint.kind==='facility'){
      const source=indices.get(t.endpoint.instance),queue=[{position:t.position,heading:null}],seen=new Set([xy(t.position)]);
      for(let i=0;i<queue.length;i++){
        const current=queue[i],p=xy(current.position);
        for(const target of sinks.get(p)??[])if(target!==source)result.add(`${source}:${target}`);
        for(const s of out.get(p)??[]){
          const direction=`${s.to.x-s.from.x},${s.to.y-s.from.y}`;
          if(bridges.has(p)&&direction!==current.heading)continue;
          const next=xy(s.to),heading=bridges.has(next)?direction:null,key=heading?`${next}/${heading}`:next;
          if(!seen.has(key)){seen.add(key);queue.push({position:s.to,heading});}
        }
      }
    }
  }
  return [...result].sort();
}
function main(o){
  const w=read(o.witness),actual=productionEdges(w).map(e=>e.join(':')).sort(),expected=directedFacilityPairs(w),
    a=new Set(actual),b=new Set(expected),extra=actual.filter(e=>!b.has(e)),missing=expected.filter(e=>!a.has(e));
  if(o['require-equal']==='true')assert.deepEqual(actual,expected);
  const result={schema_version:1,actual_edges:actual.length,channel_edges:expected.length,false_coordinate_pairs:extra.length,
    missing:missing.length,extra,expected,scope:'Physical directed facility reachability only; incoming heading cannot turn on installed bridges',
    sources:[o.witness,fileURLToPath(import.meta.url),fileURLToPath(new URL('./full_factory_poses.mjs',import.meta.url))].map(p=>({path:p,sha256:hash(p)}))};
  write(o.output,result);console.log(JSON.stringify({actual:actual.length,expected:expected.length,extra:extra.length,missing:missing.length}));
}
if(process.argv[1]&&fileURLToPath(import.meta.url)===process.argv[1])main(args(['witness','output'],['require-equal']));
