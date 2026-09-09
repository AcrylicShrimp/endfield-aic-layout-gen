"""Plot measured policy drift on fixed factory observations, not rollout rewards."""
import argparse
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

from full_factory_large_learning import read
from full_factory_screen import sha, write


def run(args):
    protocol = read(args.screen/'protocol.json')
    report = read(args.screen/'report.json')
    source = args.training/'training-result.json'
    if (report['status']!='complete' or report['protocol_sha256']!=sha(args.screen/'protocol.json')
            or protocol['sources'].get(str(source.resolve()))!=sha(source)):
        raise ValueError('on-policy.diagnosis-view-source-identity')
    training = read(source)
    figure, axes = plt.subplots(2,1,figsize=(10,7),layout='constrained')
    colors = dict(pose='#2467a6',contract='#d36b20',insert='#469562')
    warm = [r for r in report['rows'] if r['initialization']=='imitation-warm' and r['reset']=='original']
    steps = sorted({r['update'] for r in warm})
    for kind in ('pose','contract','insert'):
        values = [100*sum(r['kind_probability'].get(kind,0) for r in warm if r['update']==step)/5 for step in steps]
        axes[0].plot(steps,values,label=kind.capitalize(),color=colors[kind],linewidth=2)
    axes[0].set(title='Warm policy: mean action-kind probability on five unchanged original factories',
                ylabel='Probability (%)',ylim=(0,102),xlabel='Completed updates')
    axes[0].legend(loc='center left')
    for arm,color in [('random','#2467a6'),('imitation-warm','#d36b20')]:
        rows = [r for r in report['rows'] if r['initialization']==arm]
        steps = sorted({r['update'] for r in rows})
        values = [sum(r['total_variation_from_previous'] for r in rows if r['update']==step)/10 for step in steps]
        axes[1].plot(steps,values,label=arm,color=color,linewidth=2)
        if arm=='imitation-warm':
            flat = {g['group']+1 for g in training['groups'] if g['initialization']==arm and g['flat_returns']}
            axes[1].scatter([s for s in steps if s in flat],[v for s,v in zip(steps,values) if s in flat],
                            label='Warm: zero reward-gradient update',facecolors='white',edgecolors='#8e3159',s=35,zorder=3)
    axes[1].set(title='Change caused by each recorded update on ten unchanged factory observations',
                ylabel='Mean total variation distance',xlabel='Completed updates',ylim=(0,.29))
    axes[1].legend(loc='upper left')
    for axis in axes:
        axis.grid(alpha=.2)
    figure.suptitle('Policy movement is distinct from factory improvement',fontsize=14)
    args.output.mkdir()
    for extension in ('png','svg'):
        figure.savefig(args.output/f'policy-drift.{extension}',dpi=170)
    plt.close(figure)
    write(args.output/'view.json',dict(schema_version=1,report_sha256=sha(args.screen/'report.json'),
                                      training_result_sha256=sha(source),plot_source_sha256=sha(__file__),
                                      scope='Fixed-observation probabilities and consecutive-checkpoint total variation; not rollout scores or a causal training ablation.'))


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('screen','training','output'):
        parser.add_argument('--'+name,required=True,type=Path)
    run(parser.parse_args())
