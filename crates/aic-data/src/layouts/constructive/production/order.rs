//! Internal dependency order, independent of physical pose selection and transport routing.
use super::*;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConstructiveGrowthDecision {
    pub step: usize,
    pub root: String,
    pub variant: usize,
    pub new_members: Vec<String>,
    /// The group was routed; commit additionally requires coverage and closure audits.
    pub success: bool,
}

/// Minimal successor closure relative to an already downstream-closed incumbent.
/// For private seed initialization, pass an empty incumbent so seed cycles are included.
pub(super) fn downstream_group(
    seed: &str,
    placed: &[String],
    edges: &[&FacilityInstanceWiringEdge],
) -> BTreeSet<String> {
    let placed = placed.iter().collect::<BTreeSet<_>>();
    let mut group = BTreeSet::new();
    let mut queue = VecDeque::from([seed.to_owned()]);
    while let Some(id) = queue.pop_front() {
        if placed.contains(&id) || !group.insert(id.clone()) {
            continue;
        }
        for edge in edges.iter().filter(|e| e.source == id) {
            queue.push_back(edge.target.clone());
        }
    }
    group
}

/// Independent of the planned group: also detects missing edges between placed facilities.
pub(super) fn unclosed_outputs<'a>(
    node: &ConstructiveNode,
    edges: &[&'a FacilityInstanceWiringEdge],
) -> Vec<&'a str> {
    edges
        .iter()
        .filter(|e| {
            node.member_instances.contains(&e.source) && !node.internal_requirements.contains(&e.id)
        })
        .map(|e| e.id.as_str())
        .collect()
}
