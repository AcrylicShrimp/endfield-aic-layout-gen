//! Sequential halving against a sparse live root and a complete frozen catalog.

use std::{mem, sync::Arc};

use super::{CatalogChoice, CatalogVisit};
use crate::hierarchical_search::{
    AllowanceStop, Config, Counters, Factory, GumbelControls, MacroAction, Node,
    RootShortlistEvidence, SearchMemoryBudget, TreeReservation, capture_tree_memory_stop,
    finite_root_q_scale, placement_host_stop, simulate,
};

use super::collector::{InitialRootCatalog, SparseRootEdges};

struct SearchChoice {
    catalog: CatalogChoice,
    edge: usize,
    identity: Vec<u8>,
}

pub(super) enum SparseSearchStart {
    Ready(SparseGumbelSearch),
    MemoryStop(AllowanceStop),
}

pub(super) enum SparseSearchStep {
    Running,
    Complete,
    MemoryStop(AllowanceStop),
}

pub(super) struct SparseGumbelSearch {
    nodes: Vec<Node>,
    _edges_memory: TreeReservation,
    _choice_memory: TreeReservation,
    remaining: Vec<SearchChoice>,
    initial_count: u64,
    initial_root_value: f64,
    simulations: usize,
    visits_each: usize,
    round_choice: usize,
    round_visit: usize,
}

pub(super) struct RootCatalogSimulation {
    pub(super) selected: Option<MacroAction>,
    pub(super) selected_id: Option<Vec<u8>>,
    pub(super) selected_rank: Option<u64>,
    pub(super) selected_ordinal: Option<u64>,
    pub(super) shortlist: Vec<RootShortlistEvidence>,
    pub(super) visits: Vec<CatalogVisit>,
    pub(super) simulations: usize,
    pub(super) _edges_memory: TreeReservation,
    pub(super) _choice_memory: TreeReservation,
}

impl SparseGumbelSearch {
    pub(super) fn completed_simulations(&self) -> usize {
        self.simulations
    }

    #[cfg(test)]
    pub(super) fn root_memory_bytes(&self) -> usize {
        self.nodes[0].memory.bytes()
    }

    pub(super) fn new(
        catalog: &InitialRootCatalog<'_>,
        sparse: &mut Option<SparseRootEdges>,
        budget: &SearchMemoryBudget,
    ) -> SparseSearchStart {
        let root_bytes = catalog
            .source
            .clone_reservation_bytes()
            .saturating_add(mem::size_of::<Node>());
        let root_memory = match TreeReservation::new(budget, root_bytes) {
            Ok(memory) => memory,
            Err(stop) => return SparseSearchStart::MemoryStop(stop),
        };
        let sparse_ref = sparse.as_ref().expect("sparse edges are prepared");
        let choice_bytes = catalog
            .shortlist
            .iter()
            .enumerate()
            .fold(0usize, |bytes, (edge, _)| {
                bytes
                    .saturating_add(mem::size_of::<SearchChoice>() + 32)
                    .saturating_add(sparse_ref.edges[edge].identity.len())
            });
        let choice_memory = match TreeReservation::new(budget, choice_bytes) {
            Ok(memory) => memory,
            Err(stop) => return SparseSearchStart::MemoryStop(stop),
        };
        let mut remaining = Vec::new();
        if remaining
            .try_reserve_exact(catalog.shortlist.len())
            .is_err()
        {
            return SparseSearchStart::MemoryStop(placement_host_stop(budget, choice_bytes));
        }
        remaining.extend(
            catalog
                .shortlist
                .iter()
                .copied()
                .enumerate()
                .map(|(edge, catalog)| SearchChoice {
                    catalog,
                    edge,
                    identity: sparse_ref.edges[edge].identity.clone(),
                }),
        );
        let sparse = sparse.take().expect("sparse edges are prepared");
        let node = Node {
            session: Arc::clone(&catalog.source),
            memory: root_memory,
            visits: 0,
            value: Some(catalog.initial_root_value),
            actions: Some(sparse.edges),
            placement_pages: None,
            lower: None,
            lower_loaded_pages: 0,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        };
        SparseSearchStart::Ready(Self {
            nodes: vec![node],
            _edges_memory: sparse._memory,
            _choice_memory: choice_memory,
            remaining,
            initial_count: catalog.files.count,
            initial_root_value: catalog.initial_root_value,
            simulations: 0,
            visits_each: 0,
            round_choice: 0,
            round_visit: 0,
        })
    }

    /// One completed simulation is one resumable unit. A lower-witness memory
    /// stop retains the uncommitted update and this search owner for retry.
    pub(super) fn step(
        &mut self,
        catalog: &mut InitialRootCatalog<'_>,
        factory: &Factory<'_>,
        config: &Config,
        controls: GumbelControls,
        counters: &mut Counters,
        budget: &SearchMemoryBudget,
    ) -> anyhow::Result<SparseSearchStep> {
        assert!(
            !self.remaining.is_empty(),
            "sparse Gumbel root has no shortlist"
        );
        if self.remaining.len() == 1 && self.simulations == config.simulations_per_move {
            return Ok(SparseSearchStep::Complete);
        }
        if self.remaining.len() > 1 && self.visits_each == 0 {
            let rounds_left = self.remaining.len().ilog2() as usize
                + usize::from(!self.remaining.len().is_power_of_two());
            let available = config.simulations_per_move - self.simulations;
            self.visits_each = (available / (self.remaining.len() * rounds_left)).max(1);
        }
        if let Some(stop) =
            catalog.advance_late_lower(&mut self.nodes[0], factory, counters, budget)?
        {
            capture_tree_memory_stop(counters, catalog.source.revision(), budget, stop.clone());
            return Ok(SparseSearchStep::MemoryStop(stop));
        }
        let current = if self.remaining.len() > 1 {
            self.round_choice
        } else {
            0
        };
        let identity = &self.remaining[current].identity;
        simulate(
            &mut self.nodes,
            0,
            Some(identity),
            factory,
            config,
            counters,
            budget,
        )?;
        self.simulations += 1;
        if self.remaining.len() > 1 {
            self.round_visit += 1;
            if self.round_visit == self.visits_each {
                self.round_visit = 0;
                self.round_choice += 1;
            }
            if self.round_choice == self.remaining.len() {
                let q = self.transformed_q(controls);
                self.remaining.sort_by(|left, right| {
                    let score = |choice: &SearchChoice| {
                        let prior = choice.catalog.prior * (1.0 - controls.anchor_policy_mass)
                            + choice.catalog.anchor_prior * controls.anchor_policy_mass;
                        f64::from_bits(choice.catalog.gumbel_bits)
                            + prior.max(f64::MIN_POSITIVE).ln()
                            + q[choice.edge]
                    };
                    score(right)
                        .total_cmp(&score(left))
                        .then_with(|| left.catalog.rank.cmp(&right.catalog.rank))
                });
                self.remaining.truncate(self.remaining.len().div_ceil(2));
                self.round_choice = 0;
                self.visits_each = 0;
            }
        }
        Ok(
            if self.remaining.len() == 1 && self.simulations == config.simulations_per_move {
                SparseSearchStep::Complete
            } else {
                SparseSearchStep::Running
            },
        )
    }

    fn transformed_q(&self, controls: GumbelControls) -> Vec<f64> {
        let edges = self.nodes[0]
            .actions
            .as_ref()
            .expect("sparse root edges exist");
        let visits = edges.iter().map(|edge| u64::from(edge.visits)).sum::<u64>();
        let value = edges.iter().map(|edge| edge.value_sum).sum::<f64>();
        let mixed = (self.initial_root_value + value) / (1.0 + visits as f64);
        let mut minimum = self.initial_root_value;
        let mut maximum = self.initial_root_value;
        if edges.iter().filter(|edge| edge.visits > 0).count() < self.initial_count as usize {
            minimum = minimum.min(mixed);
            maximum = maximum.max(mixed);
        }
        for edge in edges {
            if edge.visits > 0 {
                let q = edge.value_sum / f64::from(edge.visits);
                minimum = minimum.min(q);
                maximum = maximum.max(q);
            }
        }
        let max_visits = edges.iter().map(|edge| edge.visits).max().unwrap_or(0);
        let scale = finite_root_q_scale(controls.max_visit_init, max_visits, controls.value_scale);
        edges
            .iter()
            .map(|edge| {
                let q = if edge.visits == 0 {
                    mixed
                } else {
                    edge.value_sum / f64::from(edge.visits)
                };
                if maximum - minimum <= 1e-8 {
                    0.0
                } else {
                    (q - minimum) / (maximum - minimum) * scale
                }
            })
            .collect()
    }

    pub(super) fn finish(self, catalog: &InitialRootCatalog<'_>) -> RootCatalogSimulation {
        assert_eq!(self.remaining.len(), 1, "Gumbel halving unfinished");
        let selected = &self.remaining[0];
        let selected_edge = selected.edge;
        let selected_rank = selected.catalog.rank;
        let selected_ordinal = selected.catalog.ordinal;
        let root = self.nodes.into_iter().next().expect("sparse root exists");
        let actions = root.actions.expect("sparse root edges exist");
        let mut selected_action = None;
        let mut selected_id = None;
        let mut shortlist = Vec::with_capacity(actions.len());
        let mut visits = Vec::new();
        for (index, edge) in actions.into_iter().enumerate() {
            if index == selected_edge && edge.blocked_value.is_none() {
                selected_id = Some(edge.identity.clone());
                selected_action = Some(edge.action);
            }
            if edge.visits > 0 {
                visits.push(CatalogVisit {
                    ordinal: catalog.shortlist[index].ordinal,
                    visits: edge.visits,
                    value_sum: edge.value_sum,
                });
            }
            shortlist.push(RootShortlistEvidence {
                physical_id: edge.identity,
                initial_rank: catalog.shortlist[index].rank as usize,
                gumbel_bits: catalog.shortlist[index].gumbel_bits,
            });
        }
        let selected_is_live = selected_action.is_some();
        RootCatalogSimulation {
            selected: selected_action,
            selected_id,
            selected_rank: selected_is_live.then_some(selected_rank),
            selected_ordinal: selected_is_live.then_some(selected_ordinal),
            shortlist,
            visits,
            simulations: self.simulations,
            _edges_memory: self._edges_memory,
            _choice_memory: self._choice_memory,
        }
    }
}
