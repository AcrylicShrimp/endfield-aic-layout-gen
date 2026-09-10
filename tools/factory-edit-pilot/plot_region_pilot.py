"""Render the paired region pilot summary as a standalone scientific figure."""
import argparse
import json
from pathlib import Path


def plot(source, output):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    data = json.loads(source.read_text())
    points = data['evaluation']
    batches = [p['batch'] for p in points]
    fig, axes = plt.subplots(2, 2, figsize=(12, 8), layout='constrained')
    colors = {'baseline': '#526985', 'region': '#d06921'}
    for label in ('baseline', 'region'):
        name = 'Previous executor' if label == 'baseline' else 'Region executor'
        axes[0, 0].plot(batches, [p[label + '_mean'] for p in points], 'o-', color=colors[label], label=name)
        axes[0, 1].plot(batches, [p[label + '_learning_delta_pp'] for p in points], 'o-', color=colors[label], label=name)
        for batch, style in ((0, '--'), (data['batches'], '-')):
            curve = data['trajectories'][f'{label}-{batch}']
            axes[1, 0].plot(range(len(curve)), curve, linestyle=style, color=colors[label],
                            label=f'{name}, batch {batch}', alpha=.65 if batch == 0 else 1.)
    final = points[-1]
    pairs = sorted(final['pairs'], key=lambda p: p['region'] - p['baseline'])
    delta = [p['region'] - p['baseline'] for p in pairs]
    axes[1, 1].bar(range(len(pairs)), delta, color=[colors['region'] if d >= 0 else colors['baseline'] for d in delta])
    axes[0, 0].set(title='Held-out best-area reduction', xlabel='Completed training batches', ylabel='Mean reduction from initial factory (%)')
    axes[0, 1].set(title='Change from each executor\'s untrained policy', xlabel='Completed training batches', ylabel='Mean change (percentage points)')
    axes[1, 0].set(title='Paired 32-action improvement trajectories', xlabel='Selected action', ylabel='Mean best-area reduction (%)')
    axes[1, 1].set(title=f'Batch {data["batches"]}: {final["wins"]} wins / {final["ties"]} ties / {final["losses"]} losses',
                    xlabel='Held-out factories sorted by paired difference', ylabel='Region minus previous (percentage points)')
    axes[0, 1].axhline(0, color='#999999', linewidth=.8)
    axes[1, 1].axhline(0, color='#999999', linewidth=.8)
    for axis in axes.flat:
        axis.grid(axis='y', alpha=.18)
        axis.spines[['top', 'right']].set_visible(False)
    axes[0, 0].legend(frameon=False, fontsize=9)
    axes[1, 0].legend(frameon=False, fontsize=8)
    fig.suptitle(f'Transactional route regions | {len(data["cohort"])} identical held-out factories | one seed, {data["batches"]} batches')
    output.mkdir(parents=True, exist_ok=True)
    for extension in ('png', 'svg'):
        fig.savefig(output / f'region-pilot.{extension}', dpi=160)
    plt.close(fig)
    plot_contributions(data, output)
    plot_future_credit(data, output)


def plot_contributions(data, output):
    import matplotlib.pyplot as plt
    rows = data['action_contributions']
    kinds = ['contract', 'pose_single', 'pose_group', 'insert', 'restore_best',
             'port', 'reroute', 'component', 'release_bridge']
    names = ['Contract empty strips', 'Move / rotate one facility', 'Move / rotate a group',
             'Insert strips', 'Restore best', 'Change port', 'Reroute', 'Move component', 'Release bridge']
    colors = ['#526985', '#d06921', '#4b967f', '#9467bd', '#8c564b', '#e377c2', '#7f7f7f', '#bcbd22', '#17becf']
    fig, axes = plt.subplots(2, 2, figsize=(14, 10), layout='constrained', gridspec_kw={'height_ratios': [1, 1.25]})
    active = [kind for kind in kinds if any(row[label].get(kind, {}).get('area_reduction_pp', 0) > 0
                                           for row in rows for label in ('baseline', 'region'))]
    ceiling = max(sum(v['area_reduction_pp'] for v in row[label].values()) for row in rows for label in ('baseline', 'region')) + 8
    for column, label in enumerate(('baseline', 'region')):
        bottom = [0.] * len(rows)
        for kind in active:
            values = [row[label].get(kind, {}).get('area_reduction_pp', 0.) for row in rows]
            axes[0, column].bar(range(len(rows)), values, bottom=bottom,
                                color=colors[kinds.index(kind)], label=names[kinds.index(kind)], width=.65)
            bottom = [a + b for a, b in zip(bottom, values)]
        for i, total in enumerate(bottom):
            axes[0, column].text(i, total + .6, f'{total:.2f}', ha='center', fontsize=10)
        axes[0, column].set(xticks=range(len(rows)), xticklabels=[r['batch'] for r in rows], ylim=(0, ceiling),
                            xlabel='Completed training batches', ylabel='Mean best-area reduction (%)',
                            title='Previous executor' if label == 'baseline' else 'Region executor')
    axes[0, 0].legend(frameon=False, loc='upper left', fontsize=9)
    final = rows[-1]
    delta = [final['region'].get(k, {}).get('area_reduction_pp', 0.) -
             final['baseline'].get(k, {}).get('area_reduction_pp', 0.) for k in kinds]
    axes[1, 0].barh(range(len(kinds)), delta, color=colors)
    for i, value in enumerate(delta):
        axes[1, 0].text(value + .04 if value >= 0 else value - .04, i, f'{value:+.2f}' if value else '0.00',
                        va='center', ha='left' if value >= 0 else 'right', fontsize=10)
    axes[1, 0].set(yticks=range(len(kinds)), yticklabels=names, xlabel='Region minus previous (percentage points)',
                    title=f'Batch {data["batches"]}: decomposition of the {sum(delta):+.2f} pp difference')
    axes[1, 0].set_xlim(min(0, min(delta)) - .1, max(delta) + .5)
    axes[1, 0].invert_yaxis()
    axes[1, 0].axvline(0, color='#999999', linewidth=.8)
    for label, offset, color in [('baseline', -.19, '#526985'), ('region', .19, '#d06921')]:
        counts = [final[label].get(k, {}).get('selected', 0) for k in kinds]
        axes[1, 1].barh([i + offset for i in range(len(kinds))], counts, height=.35, color=color,
                        label='Previous executor' if label == 'baseline' else 'Region executor')
    axes[1, 1].set(yticks=range(len(kinds)), yticklabels=names, xlabel='Selected actions across 32 paired evaluations',
                    title=f'Batch {data["batches"]}: action usage (including zero direct gain)')
    axes[1, 1].invert_yaxis()
    axes[1, 1].legend(frameon=False, fontsize=9)
    for axis in axes.flat:
        axis.spines[['top', 'right']].set_visible(False)
    fig.suptitle('Action contributions to best-area reduction\nExact immediate-gain accounting; enabling actions receive no indirect causal credit', fontsize=14)
    for extension in ('png', 'svg'):
        fig.savefig(output / f'action-contributions.{extension}', dpi=160)
    plt.close(fig)


def plot_future_credit(data, output):
    import matplotlib.pyplot as plt
    final = data['future_action_credit'][-1]
    kinds = ['contract', 'pose_single', 'pose_group', 'insert', 'restore_best',
             'port', 'reroute', 'component', 'release_bridge']
    names = ['Contract empty strips', 'Move / rotate one facility', 'Move / rotate a group',
             'Insert strips', 'Restore best', 'Change port', 'Reroute', 'Move component', 'Release bridge']
    fig, axes = plt.subplots(2, 2, figsize=(15, 11), layout='constrained')
    for axis, key, title, xlabel in [
        (axes[0, 0], 'summed_future_gain_pp', 'All later gains credited to every preceding action', 'Accumulated credit (repeated percentage points)'),
        (axes[0, 1], 'mean_future_gain_pp', 'Average future gain per selected action', 'Future best-area reduction (percentage points)'),
        (axes[1, 0], 'mean_position_centered_pp', 'Average excess over the same action-index mean', 'Position-centered future gain (percentage points)')]:
        for label, offset, color in [('baseline', -.19, '#526985'), ('region', .19, '#d06921')]:
            values = [final[label]['categories'].get(k, {}).get(key, 0.) for k in kinds]
            bars = axis.barh([i + offset for i in range(len(kinds))], values, height=.35, color=color,
                             label='Previous executor' if label == 'baseline' else 'Region executor')
            if key == 'mean_future_gain_pp':
                labels = [f'{v:.1f} (n={final[label]["categories"].get(k, {}).get("selected", 0)})' for k, v in zip(kinds, values)]
                axis.bar_label(bars, labels=labels, padding=3, fontsize=8)
                axis.set_xlim(0, max(axis.get_xlim()[1], max(values) * 1.45))
        axis.set(yticks=range(len(kinds)), yticklabels=names, xlabel=xlabel, title=title)
        axis.invert_yaxis()
        axis.axvline(0, color='#999999', linewidth=.8)
        axis.grid(axis='x', alpha=.15)
    for label, color in [('baseline', '#526985'), ('region', '#d06921')]:
        curve = final[label]['mean_by_position']
        axes[1, 1].plot(range(1, len(curve) + 1), curve, color=color,
                        label='Previous executor' if label == 'baseline' else 'Region executor')
    axes[1, 1].set(title='Why early actions naturally receive more credit', xlabel='Selected action index',
                    ylabel='Mean remaining best-area gain (percentage points)')
    axes[1, 1].grid(alpha=.15)
    axes[1, 1].legend(frameon=False)
    axes[0, 0].legend(frameon=False, fontsize=9)
    for axis in axes.flat:
        axis.spines[['top', 'right']].set_visible(False)
    fig.suptitle(f'Future outcome credit by action | batch {data["batches"]}, {len(data["cohort"])} paired factories\n'
                 'G(t) = all best-area gains from action t onward; descriptive association, not a causal estimate', fontsize=14)
    for extension in ('png', 'svg'):
        fig.savefig(output / f'future-action-credit.{extension}', dpi=160)
    plt.close(fig)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    plot(args.summary.resolve(), args.output.resolve())
