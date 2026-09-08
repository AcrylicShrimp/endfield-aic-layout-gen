"""Plot complete physical witnesses after native original-request acceptance."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle

from full_factory_environment import FactoryRuntime, FactoryEpisode


def run(args):
    inputs=json.loads(args.inputs.read_text())
    case=next(r for r in inputs['rows'] if r['id']==args.case)
    runtime=FactoryRuntime(args.native_library,inputs['catalogs'])
    try:
        episode=FactoryEpisode(runtime,case,horizon=1)
        initial=episode.state
        with gzip.open(args.best,'rt') as stream:
            best=json.load(stream)
        acceptance=json.loads(episode.session.propose_json(json.dumps(best)))
        if not acceptance['candidate_validated']:
            raise ValueError(f'factory.view-invalid: {args.best}: {acceptance["diagnostics"]}')
        final=json.loads(episode.session.snapshot_json())
        states=[(initial['current'],initial['current_metrics'],'Original input'),
                (final['current'],final['current_metrics'],'Selected valid layout')]
        width=max(m['width'] for _,m,_ in states);height=max(m['height'] for _,m,_ in states)
        labels={p['instance']:str(i+1) for i,p in enumerate(states[0][0]['placements'])}
        items=sorted({n['item'] for w,_,_ in states for n in w['transport_networks']})
        colors={item:plt.get_cmap('tab20')(i%20) for i,item in enumerate(items)}
        fig,axes=plt.subplots(1,2,figsize=(12,7),layout='constrained')
        for ax,(w,m,title) in zip(axes,states):
            ports=runtime.rpc('describe',w)['ports']
            ax.set_xlim(-1,width+1);ax.set_ylim(height+1,-1);ax.set_aspect('equal')
            ax.add_patch(Rectangle((0,0),m['width'],m['height'],fill=False,color='#9aa4ad',linestyle='--'))
            for p in w['placements']:
                ax.add_patch(Rectangle((p['x'],p['y']),p['width'],p['height'],facecolor='#dce8f2',edgecolor='#324c61'))
                ax.text(p['x']+p['width']/2,p['y']+p['height']/2,labels[p['instance']],ha='center',va='center',fontsize=10)
            for n in w['transport_networks']:
                color=colors[n['item']];style='--' if n['transport']=='pipe' else '-'
                for p in n['cells']:
                    ax.plot(p['x']+.5,p['y']+.5,'.',color=color,markersize=2)
                for s in n['segments']:
                    ax.plot([s['from']['x']+.5,s['to']['x']+.5],[s['from']['y']+.5,s['to']['y']+.5],style,color=color,linewidth=1.5)
                for t in n['terminals']:
                    a=t['position']
                    if t['endpoint']['kind']=='facility':
                        p=next(p for p in ports[t['endpoint']['instance']] if p['id']==t['endpoint']['port'])['position']
                        ax.plot([p['x']+.5,a['x']+.5],[p['y']+.5,a['y']+.5],style,color=color,linewidth=1.5)
                    else:
                        ax.plot(a['x']+.5,a['y']+.5,'s',color=color,markersize=4)
            for c in w['logistics_components']:
                p=c['position'];mark={'splitter':'S','converger':'M','bridge':'X'}[c['kind']]
                ax.text(p['x']+.5,p['y']+.5,mark,ha='center',va='center',fontsize=8,color='#832f88',bbox=dict(facecolor='white',pad=.2,edgecolor='none'))
            ax.set_title(f'{title}\n{m["width"]} x {m["height"]} | area / transport / turns: {m["score"]}')
            ax.set_xlabel('X grid cells');ax.set_ylabel('Y grid cells')
        fig.suptitle(args.caption,fontsize=11)
        args.output.mkdir()
        fig.savefig(args.output/'comparison.png',dpi=170)
        fig.savefig(args.output/'comparison.svg')
        plt.close(fig)
        manifest=dict(schema_version=1,case=args.case,caption=args.caption,best_source=str(args.best.resolve()),
                      best_source_sha256=hashlib.sha256(args.best.read_bytes()).hexdigest(),
                      initial_metrics=initial['current_metrics'],selected_metrics=final['current_metrics'],
                      factory_validated=True,scope='Native original-request acceptance; caption identifies selection provenance')
        (args.output/'view.json').write_text(json.dumps(manifest,indent=2)+'\n')
        print(json.dumps(dict(status='complete',output=str(args.output))))
    finally:
        runtime.close()


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inputs',type=Path,required=True)
    parser.add_argument('--case',required=True)
    parser.add_argument('--best',type=Path,required=True)
    parser.add_argument('--native-library',type=Path,required=True)
    parser.add_argument('--caption',required=True)
    parser.add_argument('--output',type=Path,required=True)
    run(parser.parse_args())
