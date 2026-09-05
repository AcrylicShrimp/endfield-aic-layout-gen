function* permutations(xs){if(!xs.length)yield[];else for(let i=0;i<xs.length;i++)for(const tail of permutations(xs.filter((_,j)=>j!==i)))yield[xs[i],...tail];}
function* concatenate(groups,index=0,head=[]){if(index===groups.length)yield head;else for(const p of permutations(groups[index]))yield*concatenate(groups,index+1,[...head,...p]);}

// Exhaustive ordered-face proof, not a grid solver and not a global nonplanarity claim.
export function restrictedOrders(links,budget=100000){
  const sources=[...new Set(links.map(l=>l.source))],targets=[...new Set(links.map(l=>l.target))];
  if(!links.length||new Set(links.map(l=>l.id)).size!==links.length)throw new Error('Expected unique links');
  let examined=0,minimum=Infinity,best=null;
  for(const sourceBlocks of permutations(sources))for(const targetBlocks of permutations(targets))
    for(const source of concatenate(sourceBlocks.map(id=>links.filter(l=>l.source===id).map(l=>l.id))))
      for(const target of concatenate(targetBlocks.map(id=>links.filter(l=>l.target===id).map(l=>l.id)))){
        if(examined++>=budget)return{status:'order-budget',exhaustive:false,examined:budget,minimum_seen:minimum};
        const rank=new Map(target.map((id,i)=>[id,i])),inversions=[];
        for(let i=0;i<source.length;i++)for(let j=i+1;j<source.length;j++)if(rank.get(source[i])>rank.get(source[j]))inversions.push([source[i],source[j]]);
        if(inversions.length<minimum){minimum=inversions.length;best={sourceBlocks,targetBlocks,source,target,inversions};}
      }
  return{status:minimum?'ordered-face-crossing-required':'ordered-face-zero-crossing',exhaustive:true,examined,minimum_inversions:minimum,best,
    assumptions:['Each facility occupies a contiguous terminal bank on one of two opposing corridor faces.',
      'All listed positive-demand links remain present; only block and within-bank terminal order changes.',
      'Paths stay inside that ordered-face corridor. Other embeddings and commodity reassignment are not ruled out.'],global_infeasibility_proven:false};
}
