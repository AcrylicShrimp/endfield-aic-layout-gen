//! Bounded nearby alternatives, each routed and validated before score comparison.
use super::*;

const NEARBY_TRIAL_LIMIT: usize = 32;

fn pending_corridors(node: &ConstructiveNode, resolved: &BTreeSet<&str>) -> BTreeSet<(i64, i64)> {
    node.boundary_requirements
        .iter()
        .filter(|b| !resolved.contains(b.requirement.as_str()))
        .flat_map(|b| b.attachments.iter())
        .filter_map(|a| match a {
            ConstructiveBoundaryAttachment::NewPort { port } => Some(port),
            _ => None,
        })
        .flat_map(|p| {
            (0..network::FUTURE_PORT_ESCAPE_CELLS).map(move |d| {
                (
                    p.connection.x + d * (p.connection.x - p.position.x),
                    p.connection.y + d * (p.connection.y - p.position.y),
                )
            })
        })
        .collect()
}

fn induced<'a>(
    members: &BTreeSet<&str>,
    wiring: &'a FacilityInstanceWiringReport,
) -> BTreeSet<&'a str> {
    wiring
        .edges
        .iter()
        .filter(|e| members.contains(e.source.as_str()) && members.contains(e.target.as_str()))
        .map(|e| e.id.as_str())
        .collect()
}

pub(super) fn future_corridors_clear(
    node: &ConstructiveNode,
    wiring: &FacilityInstanceWiringReport,
) -> bool {
    let resolved = induced(
        &node.member_instances.iter().map(String::as_str).collect(),
        wiring,
    );
    let occupied = super::super::first_pipe_frontier::occupied_cells(&node.placements);
    pending_corridors(node, &resolved)
        .iter()
        .all(|p| !occupied.contains(p))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConstructivePlacementDecision {
    pub facility: String,
    pub anchor: String,
    pub variant: usize,
    pub shortlisted: usize,
    pub attempted: usize,
    pub valid: usize,
    /// Lexicographic used area, physical layer cells, route turns.
    pub first_score: Option<[u64; 3]>,
    pub selected_score: Option<[u64; 3]>,
    pub selected_ordinal: Option<usize>,
    /// Rotation and translation relative to the padded immutable input, before canonicalization.
    pub selected_pose: Option<[i64; 3]>,
    pub termination: &'static str,
}

fn score(node: &ConstructiveNode) -> Option<[u64; 3]> {
    let area = u64::try_from(node.bounds.width)
        .ok()?
        .checked_mul(u64::try_from(node.bounds.height).ok()?)?;
    let cells = node
        .transport_networks
        .iter()
        .flat_map(|n| n.cells.iter().map(move |p| (n.transport, p.x, p.y)))
        .collect::<BTreeSet<_>>()
        .len();
    let turns = node.transport_networks.iter().try_fold(0u64, |sum, n| {
        sum.checked_add(u64::try_from(network::turns(n, &node.logistics_components)).ok()?)
    })?;
    Some([area, cells.try_into().ok()?, turns])
}

/// Generate an input-size-bounded pose set, then retain only the best cheap shortlist.
fn shortlist(
    target: &ConstructiveNode,
    fresh: &ConstructiveNode,
    anchor: &str,
    variant: usize,
    margin: i64,
    facilities: &ValidatedFacilityCatalog,
    wiring: &FacilityInstanceWiringReport,
    fixed_orientation: bool,
) -> Vec<(i64, i64, i64)> {
    let parent = target
        .placements
        .iter()
        .find(|p| p.instance == anchor)
        .unwrap();
    let facility_cells = super::super::first_pipe_frontier::occupied_cells(&target.placements);
    let mut occupied = facility_cells.clone();
    let resolved = induced(
        &target
            .member_instances
            .iter()
            .chain(&fresh.member_instances)
            .map(String::as_str)
            .collect(),
        wiring,
    );
    occupied.extend(pending_corridors(target, &resolved));
    occupied.extend(
        target
            .transport_networks
            .iter()
            .flat_map(|n| n.cells.iter().map(|p| (p.x, p.y))),
    );
    let mut candidates = BTreeMap::new();
    for r in 0..4 {
        let rotation = ((r + variant) % 4) as i64 * 90;
        if fixed_orientation && rotation != 0 {
            continue;
        }
        let shape = rotate_node(fresh, rotation);
        if !facilities
            .facility(&shape.placements[0].facility)
            .unwrap()
            .allowed_rotations
            .contains(&shape.placements[0].rotation)
        {
            continue;
        }
        let w = shape.bounds.width;
        let h = shape.bounds.height;
        let fresh_corridors = pending_corridors(&shape, &resolved);
        for neighbor in &target.placements {
            for gap in [1, 2, 4] {
                let xs = [
                    neighbor.x,
                    neighbor.x + (neighbor.width - w) / 2,
                    neighbor.x + neighbor.width - w,
                ];
                let ys = [
                    neighbor.y,
                    neighbor.y + (neighbor.height - h) / 2,
                    neighbor.y + neighbor.height - h,
                ];
                let poses = xs
                    .into_iter()
                    .flat_map(|x| {
                        [
                            (x, neighbor.y - h - gap),
                            (x, neighbor.y + neighbor.height + gap),
                        ]
                    })
                    .chain(ys.into_iter().flat_map(|y| {
                        [
                            (neighbor.x - w - gap, y),
                            (neighbor.x + neighbor.width + gap, y),
                        ]
                    }));
                for (x, y) in poses {
                    if fresh_corridors
                        .iter()
                        .any(|(cx, cy)| facility_cells.contains(&(cx + x, cy + y)))
                    {
                        continue;
                    }
                    let p = &shape.placements[0];
                    if (p.x + x..p.x + x + p.width).any(|cx| {
                        (p.y + y..p.y + y + p.height).any(|cy| occupied.contains(&(cx, cy)))
                    }) {
                        continue;
                    }
                    let width = ((target.bounds.width + margin).max(x + w) - margin.min(x)) as i128;
                    let height =
                        ((target.bounds.height + margin).max(y + h) - margin.min(y)) as i128;
                    let distance = (2 * x + w - 2 * parent.x - parent.width).abs()
                        + (2 * y + h - 2 * parent.y - parent.height).abs();
                    // Rank is a proposal heuristic only. Acceptance uses the fully routed score.
                    candidates.insert((rotation, x, y), (width * height, distance, r, x, y));
                }
            }
        }
    }
    let mut candidates = candidates.into_iter().collect::<Vec<_>>();
    candidates.sort_by_key(|(_, rank)| *rank);
    candidates
        .into_iter()
        .take(NEARBY_TRIAL_LIMIT)
        .map(|(pose, _)| pose)
        .collect()
}

impl Constructor<'_> {
    pub(super) fn select_nearby(
        &mut self,
        target: &ConstructiveNode,
        fresh: &ConstructiveNode,
        anchor: &str,
        variant: usize,
        margin: i64,
    ) -> Option<ConstructiveNode> {
        let poses = shortlist(
            target,
            fresh,
            anchor,
            variant,
            margin,
            self.facilities,
            self.wiring,
            self.request.flow_direction.is_some(),
        );
        let mut decision = ConstructivePlacementDecision {
            facility: fresh.member_instances[0].clone(),
            anchor: anchor.into(),
            variant,
            shortlisted: poses.len(),
            attempted: 0,
            valid: 0,
            first_score: None,
            selected_score: None,
            selected_ordinal: None,
            selected_pose: None,
            termination: "shortlist-exhausted",
        };
        let mut best = None;
        for (ordinal, (rotation, x, y)) in poses.into_iter().enumerate() {
            if self.exhausted() {
                decision.termination = "growth-budget-exhausted";
                break;
            }
            self.attempts += 1;
            decision.attempted += 1;
            let source = translate_node(&rotate_node(fresh, rotation), x, y);
            let mut merged = merge_nodes(&source, target);
            network::normalize(&mut merged.transport_networks);
            let Some(node) = self.close_internal(canonicalize_node(&merged)) else {
                continue;
            };
            if let Err(d) = audit::validate_coverage(&node, self.wiring, self.items) {
                self.reject(d.code);
                continue;
            }
            let Some(candidate_score) = score(&node) else {
                self.reject("production-placement-score-overflow");
                continue;
            };
            decision.valid += 1;
            decision.first_score.get_or_insert(candidate_score);
            if decision.selected_score.is_none_or(|s| candidate_score < s) {
                decision.selected_score = Some(candidate_score);
                decision.selected_ordinal = Some(ordinal);
                decision.selected_pose = Some([rotation, x, y]);
                best = Some(node);
            }
            if decision.valid >= self.request.nearby_candidates {
                decision.termination = "valid-candidate-limit";
                break;
            }
        }
        self.placement_decisions.push(decision);
        best
    }
}
