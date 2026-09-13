"""Render retained-pool ranking and auditable before/after facility geometry."""
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle

from declarative_success_scoring import read, require, write


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input',required=True)
    parser.add_argument('--report',required=True)
    parser.add_argument('--output',required=True)
    args=parser.parse_args()
    data,report=read(args.input),read(args.report)
    require(data['update']==report['update'],'plot-input','Capture and report updates differ')
    out=Path(args.output);out.mkdir(parents=True,exist_ok=True)
    labels={'area':'Area only','rare_3pp':'Area + rarity','rare_3pp_center':'+ center movement',
        'rare_3pp_exterior_capped':'+ capped exterior'}
    budget=report['config']['budgets_per_stratum'][0]
    fig,axes=plt.subplots(1,2,figsize=(11,4),layout='constrained')
    for ax,stratum in zip(axes,('small','large')):
        selected=[next(r for r in report['selections'] if r['profile']==profile and
            r['stratum']==stratum and r['budget']==budget) for profile in labels]
        values=[r['place_sequences'] for r in selected]
        bars=ax.bar(range(len(values)),values,color=['#9ca3af','#60a5fa','#a78bfa','#34d399'])
        ax.bar_label(bars,padding=3)
        ax.set_xticks(range(len(values)),labels.values(),rotation=15,ha='right',fontsize=9)
        ax.set_ylim(0,max(1,max(values))*1.2)
        ax.set_title(f"{stratum.capitalize()}: {report['population'][stratum]['place_sequences']} available")
        ax.set_ylabel(f'Rearrangement sequences in top {budget}')
        ax.spines[['top','right']].set_visible(False)
    fig.suptitle(f"Frozen success pool at update {data['update']}; ranking only, no training")
    fig.savefig(out/'selection.png',dpi=180);plt.close(fig)

    features=report['features'];by_id={r['id']:r for r in data['entries']}
    criteria=[('Direct exterior release',lambda f:f['exterior_release']>0 and f['gain']>.05,
               lambda f:f['exterior_release']),
              ('Geometry overstates the final gain',lambda f:f['exterior_release']>.1 and f['gain']<.04,
               lambda f:f['exterior_release']-f['gain']),
              ('Large preparation success without exterior release',
               lambda f:f['stratum']=='large' and f['family']=='place_preparation' and f['exterior_release']==0,
               lambda f:f['gain']),
              ('Large facility-edit finish without exterior release',
               lambda f:f['stratum']=='large' and f['family']=='place_final' and f['exterior_release']==0,
               lambda f:f['gain'])]
    examples=[]
    for title,predicate,key in criteria:
        candidates=[f for f in features if predicate(f)]
        if candidates:
            example=max(candidates,key=lambda f:(key(f),f['id']))
            examples.append(dict(title=title,features=example,entry=by_id[example['id']]))
    require(bool(examples),'plot-input','No auditable geometry examples')
    fig,axes=plt.subplots(len(examples),2,figsize=(11,3.5*len(examples)),squeeze=False,layout='constrained')
    for pair,example in zip(axes,examples):
        e,f=example['entry'],example['features']
        changed={p['instance'] for p in e['places']}
        all_poses=e['before']+e['final']
        x0=min(p[1] for p in all_poses)-1;y0=min(p[2] for p in all_poses)-1
        x1=max(p[1]+p[3] for p in all_poses)+1;y1=max(p[2]+p[4] for p in all_poses)+1
        for ax,phase in zip(pair,('before','final')):
            for alias,x,y,w,h,rotation in e[phase]:
                color=('#60a5fa' if phase=='before' else '#34d399') if alias in changed else '#d1d5db'
                ax.add_patch(Rectangle((x,y),w,h,facecolor=color,edgecolor='#374151',linewidth=.7))
                ax.text(x+w/2,y+h/2,str(alias),ha='center',va='center',fontsize=7)
            ax.set(xlim=(x0,x1),ylim=(y1,y0),aspect='equal',xlabel='x',ylabel='y')
            area=e['before_area'] if phase=='before' else e['after_area']
            ax.set_title(f"{example['title']}\n{phase.capitalize()}: used area {area} tiles",fontsize=10)
        pair[1].text(1.02,.5,f"Area gain: {100*f['gain']:.2f}%\nExterior signal: {100*f['exterior_release']:.2f}%\n"
            f"Labels: {f['labels']}\n{f['family']}",transform=pair[1].transAxes,va='center',fontsize=9)
    fig.suptitle('Actual retained success segments: colored facilities received place actions\n'
        'Facility bodies only; transport omitted. Reported used area includes transport.',fontsize=12)
    fig.savefig(out/'geometry-examples.png',dpi=160);plt.close(fig)
    # Exact IDs, source label references and action order remain inspectable.
    write(out/'geometry-examples.json',examples)
    print(json.dumps(dict(output=str(out.resolve()),examples=[dict(title=e['title'],id=e['entry']['id']) for e in examples])))


if __name__=='__main__':main()
