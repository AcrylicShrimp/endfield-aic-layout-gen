//! Pick one absolute legal orientation from required interfaces, before choosing individual ports.
use super::*;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConstructiveOrientationDecision {
    pub instance: String,
    pub rotation: i64,
    pub aligned_output_groups: usize,
    pub output_groups: usize,
    pub aligned_input_groups: usize,
    pub input_groups: usize,
}

pub(super) fn choose(
    node: &ConstructiveNode,
    facilities: &ValidatedFacilityCatalog,
    direction: FacilityPortEdge,
) -> Result<ConstructiveOrientationDecision, ConstructiveFrontierDiagnostic> {
    let p = &node.placements[0];
    let mut groups = BTreeMap::<(bool, TransportKind, String), Vec<FacilityPortEdge>>::new();
    for b in &node.boundary_requirements {
        groups
            .entry((
                b.direction == FacilityPortDirection::Output,
                b.transport,
                b.item.clone(),
            ))
            .or_default()
            .extend(b.attachments.iter().map(|a| a.port().edge));
    }
    let output_groups = groups.keys().filter(|k| k.0).count();
    let input_groups = groups.len() - output_groups;
    facilities
        .facility(&p.facility)
        .into_iter()
        .flat_map(|f| &f.allowed_rotations)
        .map(|&rotation| {
            let delta = (rotation - p.rotation).rem_euclid(360);
            let aligned = |output: bool| {
                groups
                    .iter()
                    .filter(|(k, edges)| {
                        k.0 == output
                            && edges.iter().any(|e| {
                                e.rotated_clockwise(delta)
                                    == if output {
                                        direction
                                    } else {
                                        direction.rotated_clockwise(180)
                                    }
                            })
                    })
                    .count()
            };
            ConstructiveOrientationDecision {
                instance: p.instance.clone(),
                rotation,
                aligned_output_groups: aligned(true),
                output_groups,
                aligned_input_groups: aligned(false),
                input_groups,
            }
        })
        .min_by_key(|d| {
            (
                std::cmp::Reverse(d.aligned_output_groups),
                std::cmp::Reverse(d.aligned_input_groups),
                d.rotation,
            )
        })
        .ok_or_else(|| {
            failure(
                "production-flow-orientation-unavailable",
                &p.instance,
                "no catalog-legal facility rotation is available for fixed-flow construction",
            )
        })
}
