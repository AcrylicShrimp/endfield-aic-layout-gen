"""Render recorded native-validated geometry; no generation or model execution."""
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle


def plot(root, output):
    report=json.loads((root/'report.json').read_text())
    rows=[r for r in report['rows'] if r['product'] in ('food','medical')]
    fig,axes=plt.subplots(len(rows),4,figsize=(16,8),squeeze=False)
    fig.suptitle('Same generated factory, different validated random setup',fontsize=17)
    for index,row in enumerate(rows):
        witnesses=[json.loads(Path(row['case']['initial']).read_text())]
        names=['Constructor']
        for arm in row['arms']:
            witnesses.append(json.loads((root/row['product']/arm['arm']/'current.json').read_text()))
            names.append(arm['arm'])
        ids=sorted(p['instance'] for p in witnesses[0]['placements']);colors=plt.get_cmap('tab20')
        max_x=max(max([p['x']+p['width'] for p in w['placements']]+[c['x']+1 for n in w['transport_networks'] for c in n['cells']]) for w in witnesses)
        max_y=max(max([p['y']+p['height'] for p in w['placements']]+[c['y']+1 for n in w['transport_networks'] for c in n['cells']]) for w in witnesses)
        for axis,witness,name in zip(axes[index],witnesses,names,strict=True):
            cells=[c for n in witness['transport_networks'] for c in n['cells']]
            axis.scatter([c['x']+.5 for c in cells],[c['y']+.5 for c in cells],s=2,c='#c1c7d0',marker='s',linewidths=0,zorder=1)
            for p in witness['placements']:
                n=ids.index(p['instance']);axis.add_patch(Rectangle((p['x'],p['y']),p['width'],p['height'],facecolor=colors(n%20),edgecolor='#334155',lw=.5,zorder=2))
                axis.text(p['x']+p['width']/2,p['y']+p['height']/2,str(n),fontsize=5,ha='center',va='center',zorder=3)
            axis.set(xlim=(-2,max_x+2),ylim=(max_y+2,-2),aspect='equal',title=name)
            axis.tick_params(labelsize=7);axis.grid(alpha=.15)
            if name=='Constructor':axis.set_ylabel(f'{row["product"]}: {len(ids)} facilities')
    fig.text(.5,.02,'Colors and IDs match within each row. Shared scale within each row. Gray cells are transport geometry.',ha='center',fontsize=10)
    fig.tight_layout(rect=(0,.04,1,.94));fig.savefig(output,dpi=150);plt.close(fig)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();plot(args.root,args.output)
