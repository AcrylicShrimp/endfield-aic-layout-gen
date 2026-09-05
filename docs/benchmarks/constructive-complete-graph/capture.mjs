// Capture completed native reports without committing tens of megabytes of repeated history.
// This does not change a planner result or synthesize missing geometry.
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { gzipSync } from 'node:zlib';
import { createHash } from 'node:crypto';
import { join } from 'node:path';

const [reportPath,htmlPath,wiringPath,directory]=process.argv.slice(2);
if(!directory)throw new Error('usage: node capture.mjs REPORT.json HISTORY.html WIRING.json DIRECTORY');
const raw=await readFile(reportPath),html=await readFile(htmlPath);
const r=JSON.parse(raw),w=JSON.parse(await readFile(wiringPath,'utf8')),n=r.final_node;
if(!r.complete || !r.success || !r.internal_complete || r.unresolved_internal_requirements.length || r.unresolved_external_requirements.length)throw new Error('refusing to capture an incomplete result');
const facilities=new Set(w.nodes.filter(n=>n.kind==='facility').map(n=>n.id));
const placed=new Set(n.placements.map(p=>p.instance));
const resolved=new Set([...n.internal_requirements,...n.external_requirements]);
if(placed.size!==n.placements.length || placed.size!==facilities.size || [...placed].some(id=>!facilities.has(id)) || resolved.size!==n.internal_requirements.length+n.external_requirements.length || resolved.size!==w.edges.length || w.edges.some(e=>!resolved.has(e.id)))throw new Error('original wiring coverage mismatch');

// Exact rational rates, independent of Rust's rate implementation.
const gcd=(a,b)=>b===0n?a:gcd(b,a%b);
function add(a,b){const x=BigInt(a.numerator)*BigInt(b.denominator)+BigInt(b.numerator)*BigInt(a.denominator),y=BigInt(a.denominator)*BigInt(b.denominator),g=gcd(x,y);return{numerator:String(x/g),denominator:String(y/g)}}
const zero={numerator:'0',denominator:'1'};
const expected=new Map(),actual=new Map();
const insert=(map,key,rate)=>map.set(key,add(map.get(key)||zero,rate));
for(const e of w.edges){insert(expected,JSON.stringify([e.source,e.item,'output']),e.rate);insert(expected,JSON.stringify([e.target,e.item,'input']),e.rate)}
for(const net of n.transport_networks)for(const t of net.terminals)insert(actual,JSON.stringify([t.node,net.item,t.direction]),t.rate);
if(actual.size!==expected.size || [...expected].some(([k,v])=>JSON.stringify(v)!==JSON.stringify(actual.get(k))))throw new Error('original terminal rate mismatch');
const tiles=Object.fromEntries(['belt','pipe'].map(kind=>[kind,new Set(n.transport_networks.filter(n=>n.transport===kind).flatMap(n=>n.cells.map(p=>`${p.x},${p.y}`))).size]));
const components=Object.fromEntries(['bridge','splitter','converger'].map(kind=>[kind,n.logistics_components.filter(c=>c.kind===kind).length]));
const summary={schema_version:1,production_report_schema_version:r.schema_version,request:r.request,status:r.status,complete:r.complete,facilities:placed.size,internal_requirements:n.internal_requirements.length,external_requirements:n.external_requirements.length,bounds:n.bounds,used_area:n.bounds.width*n.bounds.height,physical_tiles:tiles,components,commodity_networks:n.transport_networks.length,elapsed_ms:r.elapsed_ms,astar_searches:r.steps.reduce((v,s)=>v+s.statistics.astar_searches,0),placement_attempts:r.steps.reduce((v,s)=>v+s.placement_attempts,0),steps:r.steps.map(({node,...step})=>({...step,facilities:node.placements.length,internal_requirements:node.internal_requirements.length,external_requirements:node.external_requirements.length,bounds:node.bounds})),independent_original_wiring_audit:'passed',report_sha256:createHash('sha256').update(raw).digest('hex'),html_sha256:createHash('sha256').update(html).digest('hex')};
await mkdir(directory,{recursive:true});
await writeFile(join(directory,'summary.json'),JSON.stringify(summary,null,2)+'\n');
await writeFile(join(directory,'complete-report.json.gz'),gzipSync(raw));
await writeFile(join(directory,'complete-history.html.gz'),gzipSync(html));
console.log(JSON.stringify({...summary,steps:summary.steps.length},null,2));
