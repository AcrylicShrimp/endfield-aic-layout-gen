"""Render the scripted blueprint contract evidence as a standalone figure."""
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle


def read(path):
    return json.loads(path.read_text())


def draw(ax, value, bounds, draft=False):
    colors = plt.get_cmap('tab10')
    if draft:
        for edge, state in zip(value['blueprint']['edges'], value['edges']):
            color = colors(edge['network'])
            if state['path']:
                ax.plot([p['x'] + .5 for p in state['path']], [p['y'] + .5 for p in state['path']], color=color, lw=1.6)
            else:
                a = value['nodes'][edge['source']]['position']
                b = value['nodes'][edge['target']]['position']
                ax.plot([a['x'] + .5, b['x'] + .5], [a['y'] + .5, b['y'] + .5], color=color, lw=1, ls=':', alpha=.6)
        facilities = value['facilities']
        components = value['blueprint']['components']
    else:
        for i, network in enumerate(value['transport_networks']):
            for segment in network['segments']:
                ax.plot([segment['from']['x'] + .5, segment['to']['x'] + .5],
                        [segment['from']['y'] + .5, segment['to']['y'] + .5], color=colors(i), lw=1.6)
        facilities = value['placements']
        components = value['logistics_components']
    overlap = set()
    if draft:
        for conflict in value['conflicts']:
            overlap.update(conflict.get('facilities', []))
    for i, facility in enumerate(facilities):
        x, y, width, height = [facility[k] for k in ('x', 'y', 'width', 'height')]
        collided = facility['instance'] in overlap
        ax.add_patch(Rectangle((x, y), width, height, facecolor='#ed6a5a' if collided else '#d8e4ee',
                              edgecolor='#a52d21' if collided else '#415b70', alpha=.5 if collided else .9, hatch='//' if collided else None))
        text_y = y + height / 2
        if collided:
            text_y = y + .7 + (i % 2) * 1.25
        ax.text(x + width / 2, text_y, facility['instance'], ha='center', va='center', fontsize=10, weight='bold')
    for component in components:
        ax.scatter(component['position']['x'] + .5, component['position']['y'] + .5, s=18, color='#303843', marker='s', zorder=4)
    ax.set_xlim(bounds[0], bounds[1])
    ax.set_ylim(bounds[3], bounds[2])
    ax.set_aspect('equal')
    ax.set_xlabel('x (tiles)')
    ax.set_ylabel('y (tiles)')
    ax.grid(alpha=.12)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--probe', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    recovery = args.probe / 'overlap-and-original-topology-recovery'
    original = read(recovery / 'best.json')
    overlap = read(recovery / 'overlap-observation.json')
    recovered = read(next(recovery.glob('validated-*.json')))
    smaller = read(args.probe / 'validated-area-improvement/best.json')
    report = read(args.probe / 'report.json')
    compact = next(r for r in report['scenarios'] if r['name'] == 'validated-area-improvement')
    before, after = compact['initial_score'][0], compact['best_score'][0]
    edges = len(overlap['blueprint']['edges'])
    points = []
    for witness in (original, recovered, smaller):
        points.extend((p['x'], p['y']) for n in witness['transport_networks'] for p in n['cells'])
        for p in witness['placements']:
            points.extend(((p['x'], p['y']), (p['x'] + p['width'], p['y'] + p['height'])))
    bounds = (min(x for x, _ in points) - 1, max(x for x, _ in points) + 2,
              min(y for _, y in points) - 1, max(y for _, y in points) + 2)
    fig, axes = plt.subplots(2, 2, figsize=(14, 10), layout='constrained')
    for ax, value, title, is_draft in zip(axes.flat, (original, overlap, recovered, smaller),
        (f'Original valid factory: area {before}', 'Draft overlap: invalid, original edges pending',
         f'Recovered valid factory: all {edges} original edges restored',
         f'Separate compaction probe: valid area {after} (-{100 * (before - after) / before:g}%)'),
        (False, True, False, False)):
        draw(ax, value, bounds, is_draft)
        ax.set_title(title, fontsize=11)
    fig.suptitle('Blueprint-preserving editing: scripted contract verification, not a learned policy', fontsize=14)
    fig.supxlabel('Dotted lines mark pending original obligations, not physical routes. Black squares are fixed logistics components.', fontsize=10)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.output, dpi=160)


if __name__ == '__main__':
    main()
