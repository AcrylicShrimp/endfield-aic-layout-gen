//! Bounded destination/rotation proposals for one atomic component edit.

use aic_data::layouts::FactoryEditCatalogs;
use indexmap::IndexSet;
use serde_json::{Value, json};

use super::Action;
use crate::full_factory::{poses, spatial, types::*};

pub fn actions(w: &Witness, catalogs: &FactoryEditCatalogs) -> Result<(Vec<Action>, Value)> {
    const LIMIT: usize = 32;
    const SCANS: usize = 256;
    let bounds = extent(w)?;
    let width = bounds.width();
    let total = width * bounds.height();
    let stride = spatial::stride(total);
    let bodies = spatial::Prefix::new(bounds, w.placements.iter().flat_map(body_cells));
    let mut actions = Vec::new();
    let mut examined = 0;
    let mut rejected = 0;
    for c in &w.logistics_components {
        let spec = catalogs.components.component(&c.component).ok_or_else(|| {
            Diagnostic::new(
                "factory.component-catalog",
                "Component catalog entry is absent",
                c.id.clone(),
            )
        })?;
        let occupied: IndexSet<_> = w
            .logistics_components
            .iter()
            .filter(|other| other.id != c.id && other.transport == c.transport)
            .map(|other| Point::from(&other.position))
            .chain(
                w.transport_networks
                    .iter()
                    .filter(|n| n.transport == c.transport)
                    .flat_map(|n| n.terminals.iter().map(|t| Point::from(&t.position))),
            )
            .collect();
        let origin = Point::from(&c.position);
        let mut seen = IndexSet::new();
        let mut retained = 0;
        let mut distant = 0;
        for i in 0..SCANS {
            let point = if i == 0 {
                origin
            } else if i % 2 == 1 && i / 2 < poses::LOCAL_STEPS.len() {
                let (dx, dy) = poses::LOCAL_STEPS[i / 2];
                origin.shifted(dx, dy)
            } else {
                if distant >= total {
                    break;
                }
                let offset = (distant * stride) % total;
                distant += 1;
                Point {
                    x: bounds.min_x + offset % width,
                    y: bounds.min_y + offset / width,
                }
            };
            if !seen.insert(point) {
                continue;
            }
            examined += 1;
            if bodies.occupied(point.x, point.y, 1, 1) || occupied.contains(&point) {
                rejected += 1;
                continue;
            }
            for &rotation in &spec.allowed_rotations {
                if point == origin && rotation == c.rotation {
                    continue;
                }
                actions.push(Action::Component {
                    component: c.id.clone(),
                    x: point.x,
                    y: point.y,
                    dx: point.x - origin.x,
                    dy: point.y - origin.y,
                    rotation,
                    transport: c.transport,
                });
                retained += 1;
                if retained == LIMIT {
                    break;
                }
            }
            if retained == LIMIT {
                break;
            }
        }
    }
    let report = json!({"actions": actions.len(), "destinations_examined": examined,
        "static_rejected": rejected, "candidates_per_component": LIMIT,
        "scans_per_component": SCANS});
    Ok((actions, report))
}
