//! Position from direct connection geometry, then route the first feasible local proposal.
use super::*;

const LOCAL_POSE_LIMIT: usize = 32;
const MAX_GAP: i64 = 8;
const MAX_SLIDE: i64 = 16;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConstructiveDirectPlacementTrial {
    /// Translation of the already oriented fresh node relative to the padded input node.
    pub translation: [i64; 2],
    pub endpoint_distance: i64,
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConstructiveDirectPlacementDecision {
    pub facility: String,
    pub variant: usize,
    pub consumers: Vec<String>,
    pub suppliers: Vec<String>,
    pub requirements: Vec<String>,
    /// Both neighbor roles are already present: supplier links may run against the placement axis.
    pub mixed_neighbor_roles: bool,
    pub placement_side: &'static str,
    pub generated: usize,
    pub collision_free: usize,
    pub trials: Vec<ConstructiveDirectPlacementTrial>,
    pub termination: &'static str,
}

struct Link {
    fresh: Vec<(i64, i64)>,
    peer: Vec<(i64, i64)>,
}

fn axis(direction: FacilityPortEdge) -> (i64, i64) {
    match direction {
        FacilityPortEdge::East => (1, 0),
        FacilityPortEdge::South => (0, 1),
        FacilityPortEdge::West => (-1, 0),
        FacilityPortEdge::North => (0, -1),
    }
}

fn endpoints(node: &ConstructiveNode, requirement: &str) -> Vec<(i64, i64)> {
    node.boundary_requirements
        .iter()
        .filter(|b| b.requirement == requirement)
        .flat_map(|b| &b.attachments)
        .map(|a| (a.port().connection.x, a.port().connection.y))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn proposals(
    target: &ConstructiveNode,
    fresh: &ConstructiveNode,
    wiring: &FacilityInstanceWiringReport,
    direction: FacilityPortEdge,
    variant: usize,
) -> (ConstructiveDirectPlacementDecision, Vec<([i64; 2], i64)>) {
    let id = &fresh.member_instances[0];
    let mut decision = ConstructiveDirectPlacementDecision {
        facility: id.clone(),
        variant,
        consumers: Vec::new(),
        suppliers: Vec::new(),
        requirements: Vec::new(),
        mixed_neighbor_roles: false,
        placement_side: "upstream",
        generated: 0,
        collision_free: 0,
        trials: Vec::new(),
        termination: "local-poses-exhausted",
    };
    let mut links = Vec::new();
    for e in &wiring.edges {
        let (peer, consumer) = if e.source == *id {
            (&e.target, true)
        } else if e.target == *id {
            (&e.source, false)
        } else {
            continue;
        };
        if !target.member_instances.contains(peer) {
            continue;
        }
        if consumer {
            decision.consumers.push(peer.clone());
        } else {
            decision.suppliers.push(peer.clone());
        }
        decision.requirements.push(e.id.clone());
        links.push(Link {
            fresh: endpoints(fresh, &e.id),
            peer: endpoints(target, &e.id),
        });
    }
    decision.consumers.sort();
    decision.consumers.dedup();
    decision.suppliers.sort();
    decision.suppliers.dedup();
    decision.mixed_neighbor_roles =
        !decision.consumers.is_empty() && !decision.suppliers.is_empty();
    if links.is_empty()
        || links
            .iter()
            .any(|l| l.fresh.is_empty() || l.peer.is_empty())
    {
        decision.termination = "direct-interface-unavailable";
        return (decision, Vec::new());
    }
    let upstream = !decision.consumers.is_empty();
    decision.placement_side = if upstream { "upstream" } else { "downstream" };
    let peers = if upstream {
        &decision.consumers
    } else {
        &decision.suppliers
    };
    let (dx, dy) = axis(direction);
    let along = |x: i64, y: i64| dx * x + dy * y;
    let across = |x: i64, y: i64| -dy * x + dx * y;
    let projected = |x: i64, y: i64, w: i64, h: i64| {
        let a = along(x, y);
        let b = along(x + w - 1, y + h - 1);
        (a.min(b), a.max(b))
    };
    let peer_bounds = target
        .placements
        .iter()
        .filter(|p| peers.contains(&p.instance))
        .map(|p| projected(p.x, p.y, p.width, p.height))
        .collect::<Vec<_>>();
    let face = if upstream {
        peer_bounds.iter().map(|b| b.0).min().unwrap()
    } else {
        peer_bounds.iter().map(|b| b.1).max().unwrap()
    };
    let p = &fresh.placements[0];
    let bounds = projected(p.x, p.y, p.width, p.height);
    let alignments = links
        .iter()
        .flat_map(|l| {
            l.fresh.iter().flat_map(|&(fx, fy)| {
                l.peer
                    .iter()
                    .map(move |&(px, py)| across(px, py) - across(fx, fy))
            })
        })
        .collect::<BTreeSet<_>>();
    let centers = alignments.into_iter().collect::<Vec<_>>();
    let median = centers[centers.len() / 2];
    let mut poses = BTreeMap::new();
    for gap in 1..=MAX_GAP {
        let s = if upstream {
            face - gap - 1 - bounds.1
        } else {
            face + gap + 1 - bounds.0
        };
        for center in &centers {
            for slide in -MAX_SLIDE..=MAX_SLIDE {
                let t = center + slide;
                let x = dx * s - dy * t;
                let y = dy * s + dx * t;
                let distances = links
                    .iter()
                    .map(|l| {
                        l.fresh
                            .iter()
                            .flat_map(|&(fx, fy)| {
                                l.peer
                                    .iter()
                                    .map(move |&(px, py)| (fx + x - px).abs() + (fy + y - py).abs())
                            })
                            .min()
                            .unwrap()
                    })
                    .collect::<Vec<_>>();
                poses.insert(
                    [x, y],
                    (
                        distances.iter().sum::<i64>(),
                        *distances.iter().max().unwrap(),
                        gap,
                        (t - median).abs(),
                        t,
                    ),
                );
            }
        }
    }
    decision.generated = poses.len();
    let mut ranked = poses.into_iter().collect::<Vec<_>>();
    ranked.sort_by_key(|(_, rank)| *rank);
    let mut occupied = super::super::first_pipe_frontier::occupied_cells(&target.placements);
    occupied.extend(
        target
            .transport_networks
            .iter()
            .flat_map(|n| n.cells.iter().map(|c| (c.x, c.y))),
    );
    let clear = ranked
        .into_iter()
        .filter(|([x, y], _)| {
            !(p.x + x..p.x + x + p.width)
                .any(|cx| (p.y + y..p.y + y + p.height).any(|cy| occupied.contains(&(cx, cy))))
        })
        .collect::<Vec<_>>();
    decision.collision_free = clear.len();
    (
        decision,
        clear
            .into_iter()
            .take(LOCAL_POSE_LIMIT)
            .map(|(p, rank)| (p, rank.0))
            .collect(),
    )
}

impl Constructor<'_> {
    pub(super) fn select_direct(
        &mut self,
        target: &ConstructiveNode,
        fresh: &ConstructiveNode,
        variant: usize,
    ) -> Option<ConstructiveNode> {
        let (mut decision, poses) = proposals(
            target,
            fresh,
            self.wiring,
            self.request
                .flow_direction
                .expect("validated direct-placement direction"),
            variant,
        );
        let mut accepted = None;
        for (translation, endpoint_distance) in poses {
            if self.exhausted() {
                decision.termination = "growth-budget-exhausted";
                break;
            }
            self.attempts += 1;
            let source = translate_node(fresh, translation[0], translation[1]);
            let mut merged = merge_nodes(&source, target);
            network::normalize(&mut merged.transport_networks);
            if let Some(node) = self.close_internal(canonicalize_node(&merged)) {
                match audit::validate_coverage(&node, self.wiring, self.items) {
                    Ok(()) => accepted = Some(node),
                    Err(d) => self.reject(d.code),
                }
            }
            if self.retain_history {
                decision.trials.push(ConstructiveDirectPlacementTrial {
                    translation,
                    endpoint_distance,
                    accepted: accepted.is_some(),
                });
            }
            if accepted.is_some() {
                decision.termination = "first-valid";
                break;
            }
        }
        if accepted.is_none() {
            self.reject(if decision.termination == "direct-interface-unavailable" {
                "production-direct-interface-unavailable"
            } else {
                "production-direct-placement-exhausted"
            });
        }
        if self.retain_history {
            self.direct_placement_decisions.push(decision);
        }
        accepted
    }
}
