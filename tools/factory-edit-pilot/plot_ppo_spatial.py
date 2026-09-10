"""Standalone scientific figures for the controlled spatial-reach comparison."""
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np

LABELS = {'cnn2':'2 layers / 5x5', 'cnn4':'4 layers / 9x9',
          'cnn4-dilated':'4 dilated / 17x17', 'cnn8':'8 layers / 17x17'}
COLORS = dict(zip(LABELS, ['#486a9b','#d28a24','#198c75','#a65799']))


def save(fig, output, name):
    fig.savefig(output/f'{name}.png',dpi=170,bbox_inches='tight')
    fig.savefig(output/f'{name}.svg',bbox_inches='tight')
    plt.close(fig)


def plot(summary, output):
    value=json.loads(summary.read_text())
    output.mkdir(parents=True,exist_ok=True)
    plt.rcParams.update({'font.size':10,'axes.spines.top':False,'axes.spines.right':False})
    fig,axes=plt.subplots(2,2,figsize=(13,8.5),layout='constrained')
    for name,item in value['variants'].items():
        points=item['evaluation']
        x=[p['batch'] for p in points]
        args=dict(color=COLORS[name],label=LABELS[name])
        axes[0,0].plot(x,[p['mean_area_reduction_pp'] for p in points],marker='o',**args)
        axes[0,1].plot(x,[p['learning_delta_pp'] for p in points],marker='o',**args)
        axes[1,0].plot(points[-1]['trajectory'],**args)
        if name!='cnn2':
            differences=[p['minus_cnn2_pp'] for p in points[-1]['pairs']]
            offset={'cnn4':-.22,'cnn4-dilated':0,'cnn8':.22}[name]
            axes[1,1].scatter(np.arange(len(differences))+offset,differences,s=18,alpha=.8,**args)
    axes[0,0].set(title='Best-area reduction on identical factories',xlabel='Training batch',ylabel='Mean reduction (%)')
    axes[0,1].set(title='Change from each untrained policy',xlabel='Training batch',ylabel='Improvement (percentage points)')
    axes[1,0].set(title=f'Final policy: {len(value["cohort"])} paired trajectories',xlabel='Selected action (failures included)',ylabel='Mean best-area reduction (%)')
    axes[1,1].set(title='Final per-factory difference from 2 layers',xlabel='Factory index (fixed seed order)',ylabel='Difference (percentage points)')
    axes[1,1].axhline(0,color='#777',lw=.8)
    for ax in axes.flat:
        ax.grid(alpha=.18)
    axes[0,0].legend(fontsize=9)
    fig.suptitle('Actor spatial receptive-field screen — one initialization seed',fontsize=15)
    save(fig,output,'spatial-learning')

    kinds=['contract','pose_single','pose_group','insert','restore_best','port','reroute','component','release']
    present=sorted(set().union(*(set(v['evaluation'][-1]['action_contributions_pp']) for v in value['variants'].values())))
    kinds=[k for k in kinds if k in present]+[k for k in present if k not in kinds]
    fig,axes=plt.subplots(1,2,figsize=(14,5),layout='constrained')
    x=np.arange(len(kinds))
    for i,(name,item) in enumerate(value['variants'].items()):
        final=item['evaluation'][-1]
        offset=(i-1.5)*.2
        axes[0].bar(x+offset,[final['action_contributions_pp'].get(k,0) for k in kinds],width=.19,color=COLORS[name],label=LABELS[name])
        credit=final['future_credit']['categories']
        axes[1].bar(x+offset,[credit.get(k,{}).get('mean_position_centered_pp',0) for k in kinds],width=.19,color=COLORS[name])
    for ax in axes:
        ax.set_xticks(x,kinds,rotation=45,ha='right')
        ax.grid(axis='y',alpha=.18)
    axes[0].set(title='Direct contribution to final best-area reduction',ylabel='Percentage points (additive)')
    axes[1].set(title='Future reward: centered at the same action index',ylabel='Mean percentage points per selected action')
    axes[0].legend(fontsize=9)
    axes[1].axhline(0,color='#777',lw=.8)
    fig.suptitle('Action outcomes — selected-action associations are not causal effects',fontsize=14)
    save(fig,output,'spatial-action-outcomes')

    fig,axes=plt.subplots(1,2,figsize=(12,4.5),layout='constrained')
    names=list(value['variants'])
    for ax,key,title in [(axes[0],'episode_worker_seconds','Summed collection episode worker time'),
                         (axes[1],'update_seconds','Summed PPO update time')]:
        ax.bar(np.arange(len(names)),[value['variants'][n]['training'][key] for n in names],color=[COLORS[n] for n in names])
        ax.set_xticks(np.arange(len(names)),[LABELS[n] for n in names],rotation=15,ha='right')
        ax.set(title=title,ylabel='Seconds')
        ax.grid(axis='y',alpha=.18)
    fig.suptitle(f'Cost for {value["batches"]} fresh training batches; serial runs',fontsize=14)
    save(fig,output,'spatial-cost')


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    plot(args.summary.resolve(),args.output.resolve())
