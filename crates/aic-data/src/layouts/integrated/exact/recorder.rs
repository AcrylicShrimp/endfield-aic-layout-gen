use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Deref, DerefMut};

use pumpkin_solver::Solver;
use pumpkin_solver::core::predicates::{Predicate, PredicateConstructor};
use pumpkin_solver::core::proof::ConstraintTag;
use pumpkin_solver::core::variables::{AffineView, DomainId, Literal, TransformableVariable};

use crate::research::{
    ConstraintFamilyMetrics, ConstraintRelation, ConstraintSummaryMetrics, CouplingMetrics,
    DomainCardinalitySummary, FactorGraphMetrics, FamilyIncidenceMetrics, MetricCoverage,
    ModelComplexityMetrics, VariableDomainMetrics, VariableFamilyMetrics,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum VariableFamily {
    Placement,
    PhysicalOccupancy,
    TransportOccupancy,
    Endpoint,
    EndpointGeometry,
    BoundaryTerminal,
    RouteCell,
    RouteArc,
    Flow,
    TerminalPresence,
    RouteArm,
    ArmItem,
    BranchComponent,
    Bridge,
    BridgeRotation,
    CrossingOwner,
    ConnectivityReachability,
    ConnectivityRoot,
    ConnectivityParent,
    ConnectivityDepth,
    Objective,
}

impl VariableFamily {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Placement => "placement",
            Self::PhysicalOccupancy => "physical-occupancy",
            Self::TransportOccupancy => "transport-occupancy",
            Self::Endpoint => "endpoint",
            Self::EndpointGeometry => "endpoint-geometry",
            Self::BoundaryTerminal => "boundary-terminal",
            Self::RouteCell => "route-cell",
            Self::RouteArc => "route-arc",
            Self::Flow => "flow",
            Self::TerminalPresence => "terminal-presence",
            Self::RouteArm => "route-arm",
            Self::ArmItem => "arm-item",
            Self::BranchComponent => "branch-component",
            Self::Bridge => "bridge",
            Self::BridgeRotation => "bridge-rotation",
            Self::CrossingOwner => "crossing-owner",
            Self::ConnectivityReachability => "connectivity-reachability",
            Self::ConnectivityRoot => "connectivity-root",
            Self::ConnectivityParent => "connectivity-parent",
            Self::ConnectivityDepth => "connectivity-depth",
            Self::Objective => "objective",
        }
    }

    fn placement_side(self) -> bool {
        matches!(
            self,
            Self::Placement | Self::PhysicalOccupancy | Self::Endpoint | Self::EndpointGeometry
        )
    }

    fn routing_side(self) -> bool {
        matches!(
            self,
            Self::RouteCell
                | Self::TransportOccupancy
                | Self::RouteArc
                | Self::Flow
                | Self::TerminalPresence
                | Self::RouteArm
                | Self::ArmItem
                | Self::BranchComponent
                | Self::Bridge
                | Self::BridgeRotation
                | Self::CrossingOwner
                | Self::ConnectivityReachability
                | Self::ConnectivityRoot
                | Self::ConnectivityParent
                | Self::ConnectivityDepth
                | Self::BoundaryTerminal
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ConstraintFamily {
    PlacementChoice,
    FacilityNonOverlap,
    EndpointLink,
    EndpointChoice,
    BoundaryTerminal,
    ArcActivation,
    FlowConservation,
    TerminalPresence,
    RouteArm,
    ItemAssignment,
    OpposingArms,
    LineCapacity,
    BranchTopology,
    RouteCellActivation,
    DirectedArcExclusion,
    BridgeRotation,
    BridgeCrossing,
    TransportCollision,
    OccupancyChannel,
    UsedGeometry,
    CanonicalTranslation,
    BoundingBox,
    ObjectiveDefinition,
    TurnDefinition,
    ConnectivityWitness,
    ConnectivityPropagator,
    GridAnalyzer,
    MaterialSeparator,
    MaterialJunction,
    GuardedCore,
    CrossingRestriction,
    ResearchFixation,
}

impl ConstraintFamily {
    fn name(self) -> &'static str {
        match self {
            Self::PlacementChoice => "placement-choice",
            Self::FacilityNonOverlap => "facility-non-overlap",
            Self::EndpointLink => "endpoint-link",
            Self::EndpointChoice => "endpoint-choice",
            Self::BoundaryTerminal => "boundary-terminal",
            Self::ArcActivation => "arc-activation",
            Self::FlowConservation => "flow-conservation",
            Self::TerminalPresence => "terminal-presence",
            Self::RouteArm => "route-arm",
            Self::ItemAssignment => "item-assignment",
            Self::OpposingArms => "opposing-arms",
            Self::LineCapacity => "line-capacity",
            Self::BranchTopology => "branch-topology",
            Self::RouteCellActivation => "route-cell-activation",
            Self::DirectedArcExclusion => "directed-arc-exclusion",
            Self::BridgeRotation => "bridge-rotation",
            Self::BridgeCrossing => "bridge-crossing",
            Self::TransportCollision => "transport-collision",
            Self::OccupancyChannel => "occupancy-channel",
            Self::UsedGeometry => "used-geometry",
            Self::CanonicalTranslation => "canonical-translation",
            Self::BoundingBox => "bounding-box",
            Self::ObjectiveDefinition => "objective-definition",
            Self::TurnDefinition => "turn-definition",
            Self::ConnectivityWitness => "connectivity-witness",
            Self::ConnectivityPropagator => "connectivity-propagator",
            Self::GridAnalyzer => "grid-analyzer",
            Self::MaterialSeparator => "material-separator",
            Self::MaterialJunction => "material-junction",
            Self::GuardedCore => "guarded-core",
            Self::CrossingRestriction => "crossing-restriction",
            Self::ResearchFixation => "research-fixation",
        }
    }

    fn is_objective(self) -> bool {
        matches!(
            self,
            Self::UsedGeometry
                | Self::CanonicalTranslation
                | Self::BoundingBox
                | Self::ObjectiveDefinition
                | Self::TurnDefinition
        )
    }
}

#[derive(Debug, Clone)]
struct VariableRecord {
    family: VariableFamily,
    name: String,
    lower_bound: i32,
    upper_bound: i32,
    cardinality: u64,
    degree: u64,
    parent: DomainId,
    rank: u8,
}

#[derive(Debug, Clone)]
pub(super) struct RecordedVariableDescriptor {
    pub(super) domain: DomainId,
    pub(super) family: VariableFamily,
    pub(super) name: String,
    pub(super) declared_lower_bound: i32,
    pub(super) declared_upper_bound: i32,
    pub(super) declared_cardinality: u64,
}

#[derive(Debug, Clone, Default)]
struct ConstraintAggregate {
    constraints: u64,
    terms: u64,
    arities: BTreeMap<u64, u64>,
    maximum_absolute_coefficient: u64,
}

#[derive(Debug, Default)]
struct ModelRecorder {
    variables: BTreeMap<DomainId, VariableRecord>,
    constraints: BTreeMap<(ConstraintFamily, ConstraintRelation), ConstraintAggregate>,
    all_constraints: ConstraintAggregate,
    family_incidences: BTreeMap<(VariableFamily, ConstraintFamily), u64>,
    cross_family_constraints: u64,
    placement_routing_constraints: u64,
    placement_routing_incidences: u64,
    network_collision_constraints: u64,
    objective_incidences: u64,
    facility_network_incidences: u64,
    shared_network_facility_pairs: u64,
}

pub(super) struct RecordedModel {
    solver: Solver,
    recorder: ModelRecorder,
}

impl Default for RecordedModel {
    fn default() -> Self {
        Self {
            solver: Solver::default(),
            recorder: ModelRecorder::default(),
        }
    }
}

impl Deref for RecordedModel {
    type Target = Solver;

    fn deref(&self) -> &Self::Target {
        &self.solver
    }
}

impl DerefMut for RecordedModel {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.solver
    }
}

impl RecordedModel {
    pub(super) fn solver_mut(&mut self) -> &mut Solver {
        &mut self.solver
    }

    pub(super) fn new_variable(
        &mut self,
        family: VariableFamily,
        lower_bound: i32,
        upper_bound: i32,
        name: impl Into<String>,
    ) -> DomainId {
        let name = name.into();
        let variable =
            self.solver
                .new_named_bounded_integer(lower_bound, upper_bound, name.clone());
        let cardinality = i64::from(upper_bound) - i64::from(lower_bound) + 1;
        self.recorder.variables.insert(
            variable,
            VariableRecord {
                family,
                name,
                lower_bound,
                upper_bound,
                cardinality: u64::try_from(cardinality)
                    .expect("validated integer domain has positive cardinality"),
                degree: 0,
                parent: variable,
                rank: 0,
            },
        );
        variable
    }

    pub(super) fn new_sparse_variable(
        &mut self,
        family: VariableFamily,
        values: impl Into<Vec<i32>>,
        name: impl Into<String>,
    ) -> DomainId {
        let name = name.into();
        let mut values = values.into();
        values.sort_unstable();
        values.dedup();
        assert!(
            !values.is_empty(),
            "sparse variable domain must be non-empty"
        );
        let cardinality = u64::try_from(values.len()).expect("sparse domain cardinality fits u64");
        let lower_bound = values[0];
        let upper_bound = *values.last().expect("sparse domain is non-empty");
        let variable = self.solver.new_named_sparse_integer(values, name.clone());
        self.recorder.variables.insert(
            variable,
            VariableRecord {
                family,
                name,
                lower_bound,
                upper_bound,
                cardinality,
                degree: 0,
                parent: variable,
                rank: 0,
            },
        );
        variable
    }

    pub(super) fn new_named_literal_for_predicate(
        &mut self,
        family: VariableFamily,
        predicate: Predicate,
        tag: ConstraintTag,
        name: impl Into<String>,
    ) -> Literal {
        let name = name.into();
        let literal = self
            .solver
            .new_named_literal_for_predicate(predicate, tag, name.clone());
        let variable = *literal.get_integer_variable().inner();
        self.recorder.variables.insert(
            variable,
            VariableRecord {
                family,
                name,
                lower_bound: 0,
                upper_bound: 1,
                cardinality: 2,
                degree: 0,
                parent: variable,
                rank: 0,
            },
        );
        literal
    }

    pub(super) fn post_equals(
        &mut self,
        family: ConstraintFamily,
        terms: Vec<AffineView<DomainId>>,
        rhs: i32,
        maximum_absolute_coefficient: u64,
        tag: ConstraintTag,
    ) {
        self.record_constraint(
            family,
            ConstraintRelation::Equality,
            &terms,
            maximum_absolute_coefficient,
        );
        self.solver
            .add_constraint(pumpkin_solver::equals(terms, rhs, tag))
            .post();
    }

    pub(super) fn post_less_than_or_equals(
        &mut self,
        family: ConstraintFamily,
        terms: Vec<AffineView<DomainId>>,
        rhs: i32,
        maximum_absolute_coefficient: u64,
        tag: ConstraintTag,
    ) {
        self.record_constraint(
            family,
            ConstraintRelation::LessThanOrEqual,
            &terms,
            maximum_absolute_coefficient,
        );
        self.solver
            .add_constraint(pumpkin_solver::less_than_or_equals(terms, rhs, tag))
            .post();
    }

    pub(super) fn post_greater_than_or_equals(
        &mut self,
        family: ConstraintFamily,
        terms: Vec<AffineView<DomainId>>,
        rhs: i32,
        maximum_absolute_coefficient: u64,
        tag: ConstraintTag,
    ) {
        self.record_constraint(
            family,
            ConstraintRelation::GreaterThanOrEqual,
            &terms,
            maximum_absolute_coefficient,
        );
        self.solver
            .add_constraint(pumpkin_solver::greater_than_or_equals(terms, rhs, tag))
            .post();
    }

    pub(super) fn post_implied_less_than_or_equals(
        &mut self,
        family: ConstraintFamily,
        terms: Vec<AffineView<DomainId>>,
        rhs: i32,
        maximum_absolute_coefficient: u64,
        condition: Literal,
        condition_parent: DomainId,
        tag: ConstraintTag,
    ) {
        let mut recorded_terms = terms.clone();
        recorded_terms.push(condition_parent.scaled(1));
        self.record_constraint(
            family,
            ConstraintRelation::LessThanOrEqual,
            &recorded_terms,
            maximum_absolute_coefficient,
        );
        self.solver
            .add_constraint(pumpkin_solver::less_than_or_equals(terms, rhs, tag))
            .implied_by(condition);
    }

    pub(super) fn post_implied_equals(
        &mut self,
        family: ConstraintFamily,
        terms: Vec<AffineView<DomainId>>,
        rhs: i32,
        maximum_absolute_coefficient: u64,
        condition: Literal,
        condition_parent: DomainId,
        tag: ConstraintTag,
    ) {
        let mut recorded_terms = terms.clone();
        recorded_terms.push(condition_parent.scaled(1));
        self.record_constraint(
            family,
            ConstraintRelation::Equality,
            &recorded_terms,
            maximum_absolute_coefficient,
        );
        self.solver
            .add_constraint(pumpkin_solver::equals(terms, rhs, tag))
            .implied_by(condition);
    }

    pub(super) fn post_implied_value_equals_clause(
        &mut self,
        family: ConstraintFamily,
        variable_family: VariableFamily,
        variable: DomainId,
        value: i32,
        condition: Literal,
        condition_parent: DomainId,
        tag: ConstraintTag,
    ) {
        self.record_constraint(
            family,
            ConstraintRelation::Equality,
            &[variable.scaled(1), condition_parent.scaled(1)],
            value.unsigned_abs() as u64,
        );
        let required_value = self.new_named_literal_for_predicate(
            variable_family,
            variable.equality_predicate(value),
            tag,
            format!("guarded-value-{variable:?}-{value}"),
        );
        self.solver.add_clause(
            [
                condition.get_false_predicate(),
                required_value.get_true_predicate(),
            ],
            tag,
        );
    }

    pub(super) fn post_implied_binary_equals(
        &mut self,
        family: ConstraintFamily,
        left: DomainId,
        right: DomainId,
        condition: Literal,
        condition_parent: DomainId,
        tag: ConstraintTag,
    ) {
        self.record_constraint(
            family,
            ConstraintRelation::Equality,
            &[left.scaled(1), right.scaled(-1), condition_parent.scaled(1)],
            1,
        );
        self.solver
            .add_constraint(pumpkin_solver::binary_equals(left, right, tag))
            .implied_by(condition);
    }

    pub(super) fn post_maximum(
        &mut self,
        family: ConstraintFamily,
        terms: Vec<AffineView<DomainId>>,
        result: DomainId,
        maximum_absolute_coefficient: u64,
        tag: ConstraintTag,
    ) {
        let mut recorded_terms = terms.clone();
        recorded_terms.push(AffineView::new(result, 1, 0));
        self.record_constraint(
            family,
            ConstraintRelation::Maximum,
            &recorded_terms,
            maximum_absolute_coefficient,
        );
        self.solver
            .add_constraint(pumpkin_solver::maximum(terms, result, tag))
            .post();
    }

    pub(super) fn post_times(
        &mut self,
        family: ConstraintFamily,
        left: DomainId,
        right: DomainId,
        result: DomainId,
        tag: ConstraintTag,
    ) {
        self.record_constraint(
            family,
            ConstraintRelation::Multiplication,
            &[
                AffineView::new(left, 1, 0),
                AffineView::new(right, 1, 0),
                AffineView::new(result, 1, 0),
            ],
            1,
        );
        self.solver
            .add_constraint(pumpkin_solver::times(left, right, result, tag))
            .post();
    }

    pub(super) fn post_table(
        &mut self,
        family: ConstraintFamily,
        variables: Vec<DomainId>,
        rows: Vec<Vec<i32>>,
        tag: ConstraintTag,
    ) {
        let terms = variables
            .iter()
            .copied()
            .map(|variable| AffineView::new(variable, 1, 0))
            .collect::<Vec<_>>();
        self.record_constraint(family, ConstraintRelation::Other, &terms, 1);
        self.solver
            .add_constraint(pumpkin_solver::table(variables, rows, tag))
            .post();
    }

    pub(super) fn post_predicate_clause(
        &mut self,
        family: ConstraintFamily,
        variables: &[DomainId],
        predicates: Vec<Predicate>,
        tag: ConstraintTag,
    ) {
        assert_eq!(
            variables.len(),
            predicates.len(),
            "recorded predicate clause variables must match its predicates"
        );
        let terms = variables
            .iter()
            .copied()
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>();
        self.record_constraint(family, ConstraintRelation::Other, &terms, 1);
        self.solver.add_clause(predicates, tag);
    }

    /// Posts the same non-reified positive-table encoding as Pumpkin 0.5 while retaining the
    /// generated row-selector domains for research-only search provenance measurement.
    pub(super) fn post_tracked_table(
        &mut self,
        family: ConstraintFamily,
        variables: Vec<DomainId>,
        rows: Vec<Vec<i32>>,
        tag: ConstraintTag,
    ) -> Vec<DomainId> {
        let terms = variables
            .iter()
            .copied()
            .map(|variable| AffineView::new(variable, 1, 0))
            .collect::<Vec<_>>();
        self.record_constraint(family, ConstraintRelation::Other, &terms, 1);

        let selectors = (0..rows.len())
            .map(|_| self.solver.new_literal())
            .collect::<Vec<_>>();
        for (column, variable) in variables.iter().enumerate() {
            let mut supports_by_value = BTreeMap::<i32, Vec<Literal>>::new();
            for (row, selector) in selectors.iter().copied().enumerate() {
                supports_by_value
                    .entry(rows[row][column])
                    .or_default()
                    .push(selector);
            }
            for (value, supports) in supports_by_value {
                let condition = variable.equality_predicate(value);
                for support in &supports {
                    self.solver
                        .add_clause(vec![support.get_false_predicate(), condition], tag);
                }
                let mut clause = vec![!condition];
                clause.extend(supports.iter().map(|support| support.get_true_predicate()));
                self.solver.add_clause(clause, tag);
            }
        }
        self.solver
            .add_constraint(pumpkin_solver::clause(selectors.clone(), tag))
            .post();
        selectors
            .iter()
            .map(|selector| *selector.get_integer_variable().inner())
            .collect()
    }

    pub(super) fn post_constant_element(
        &mut self,
        family: ConstraintFamily,
        index: DomainId,
        values: Vec<i32>,
        result: DomainId,
        tag: ConstraintTag,
    ) {
        let maximum_absolute_coefficient = values
            .iter()
            .map(|value| value.unsigned_abs() as u64)
            .max()
            .unwrap_or(1)
            .max(1);
        self.record_constraint(
            family,
            ConstraintRelation::Other,
            &[index.scaled(1), result.scaled(1)],
            maximum_absolute_coefficient,
        );
        self.solver
            .add_constraint(pumpkin_solver::element(index, values, result, tag))
            .post();
    }

    pub(super) fn post_variable_element(
        &mut self,
        family: ConstraintFamily,
        index: DomainId,
        values: Vec<DomainId>,
        result: DomainId,
        tag: ConstraintTag,
    ) {
        let mut terms = Vec::with_capacity(values.len() + 2);
        terms.push(index.scaled(1));
        terms.extend(values.iter().copied().map(|value| value.scaled(1)));
        terms.push(result.scaled(1));
        self.record_constraint(family, ConstraintRelation::Other, &terms, 1);
        self.solver
            .add_constraint(pumpkin_solver::element(index, values, result, tag))
            .post();
    }

    pub(super) fn set_logical_coupling(
        &mut self,
        facility_network_incidences: u64,
        shared_network_facility_pairs: u64,
    ) {
        self.recorder.facility_network_incidences = facility_network_incidences;
        self.recorder.shared_network_facility_pairs = shared_network_facility_pairs;
    }

    pub(super) fn record_global_constraint(
        &mut self,
        family: ConstraintFamily,
        variables: impl IntoIterator<Item = DomainId>,
    ) {
        let terms = variables
            .into_iter()
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>();
        self.record_constraint(family, ConstraintRelation::Other, &terms, 1);
    }

    pub(super) fn metrics(&mut self) -> ModelComplexityMetrics {
        self.recorder.metrics()
    }

    pub(super) fn variable_catalog(&self) -> Vec<RecordedVariableDescriptor> {
        self.recorder
            .variables
            .iter()
            .map(|(domain, record)| RecordedVariableDescriptor {
                domain: *domain,
                family: record.family,
                name: record.name.clone(),
                declared_lower_bound: record.lower_bound,
                declared_upper_bound: record.upper_bound,
                declared_cardinality: record.cardinality,
            })
            .collect()
    }

    pub(super) fn variable_descriptor(&self, domain: DomainId) -> RecordedVariableDescriptor {
        let record = self
            .recorder
            .variables
            .get(&domain)
            .unwrap_or_else(|| panic!("unrecorded model variable {}", domain.id()));
        RecordedVariableDescriptor {
            domain,
            family: record.family,
            name: record.name.clone(),
            declared_lower_bound: record.lower_bound,
            declared_upper_bound: record.upper_bound,
            declared_cardinality: record.cardinality,
        }
    }

    fn record_constraint(
        &mut self,
        family: ConstraintFamily,
        relation: ConstraintRelation,
        terms: &[AffineView<DomainId>],
        maximum_absolute_coefficient: u64,
    ) {
        let variables = terms
            .iter()
            .map(|term| *term.inner())
            .collect::<BTreeSet<_>>();
        let arity = variables.len() as u64;
        let term_count = terms.len() as u64;
        record_aggregate(
            &mut self.recorder.all_constraints,
            arity,
            term_count,
            maximum_absolute_coefficient,
        );
        record_aggregate(
            self.recorder
                .constraints
                .entry((family, relation))
                .or_default(),
            arity,
            term_count,
            maximum_absolute_coefficient,
        );

        let variable_families = variables
            .iter()
            .map(|variable| {
                self.recorder
                    .variables
                    .get(variable)
                    .unwrap_or_else(|| panic!("unrecorded model variable {}", variable.id()))
                    .family
            })
            .collect::<BTreeSet<_>>();
        if variable_families.len() > 1 {
            self.recorder.cross_family_constraints += 1;
        }
        let placement_routing = variable_families
            .iter()
            .any(|family| family.placement_side())
            && variable_families.iter().any(|family| family.routing_side());
        if placement_routing {
            self.recorder.placement_routing_constraints += 1;
            self.recorder.placement_routing_incidences += arity;
        }
        if matches!(
            family,
            ConstraintFamily::TransportCollision | ConstraintFamily::BridgeCrossing
        ) {
            self.recorder.network_collision_constraints += 1;
        }
        if family.is_objective() {
            self.recorder.objective_incidences += arity;
        }

        for variable in &variables {
            let record = self
                .recorder
                .variables
                .get_mut(variable)
                .expect("constraint variables are recorded before posting");
            record.degree += 1;
            *self
                .recorder
                .family_incidences
                .entry((record.family, family))
                .or_default() += 1;
        }
        if let Some(first) = variables.first().copied() {
            for variable in variables.iter().copied().skip(1) {
                union(&mut self.recorder.variables, first, variable);
            }
        }
    }
}

impl ModelRecorder {
    fn metrics(&mut self) -> ModelComplexityMetrics {
        let mut by_family = BTreeMap::<VariableFamily, Vec<u64>>::new();
        let mut log2_domain_volume = 0.0;
        let mut boolean_variables = 0_u64;
        let mut integer_variables = 0_u64;
        let mut degrees = Vec::with_capacity(self.variables.len());
        for variable in self.variables.values() {
            by_family
                .entry(variable.family)
                .or_default()
                .push(variable.cardinality);
            log2_domain_volume += (variable.cardinality as f64).log2();
            if variable.cardinality == 2 {
                boolean_variables += 1;
            } else {
                integer_variables += 1;
            }
            degrees.push(variable.degree);
        }
        let variable_families = by_family
            .into_iter()
            .map(|(family, cardinalities)| variable_family_metrics(family, cardinalities))
            .collect::<Vec<_>>();

        let constraints = self
            .constraints
            .iter()
            .map(|((family, relation), aggregate)| ConstraintFamilyMetrics {
                family: family.name().to_string(),
                relation: *relation,
                constraints: aggregate.constraints,
                terms: aggregate.terms,
                maximum_arity: maximum_observed(&aggregate.arities),
                p95_arity: percentile_histogram(&aggregate.arities, 95),
                maximum_absolute_coefficient: aggregate.maximum_absolute_coefficient,
            })
            .collect::<Vec<_>>();
        let constraint_summary = ConstraintSummaryMetrics {
            total_constraints: self.all_constraints.constraints,
            total_terms: self.all_constraints.terms,
            maximum_arity: maximum_observed(&self.all_constraints.arities),
            p95_arity: percentile_histogram(&self.all_constraints.arities, 95),
            maximum_absolute_coefficient: self.all_constraints.maximum_absolute_coefficient,
            by_family: constraints,
        };

        let constraint_degrees = expand_histogram(&self.all_constraints.arities);
        let variable_count = self.variables.len() as u64;
        let constraint_count = self.all_constraints.constraints;
        let incidences = degrees.iter().sum::<u64>();
        let components = connected_components(&mut self.variables);
        let factor_graph = FactorGraphMetrics {
            variable_vertices: variable_count,
            constraint_vertices: constraint_count,
            incidences,
            mean_variable_degree: mean(&degrees),
            maximum_variable_degree: degrees.iter().copied().max().unwrap_or(0),
            p95_variable_degree: percentile(&degrees, 95),
            mean_constraint_degree: mean(&constraint_degrees),
            maximum_constraint_degree: constraint_degrees.iter().copied().max().unwrap_or(0),
            p95_constraint_degree: percentile(&constraint_degrees, 95),
            density: if variable_count == 0 || constraint_count == 0 {
                0.0
            } else {
                incidences as f64 / (variable_count as f64 * constraint_count as f64)
            },
            connected_components: Some(components),
            articulation_points: None,
            retained_full_graph: false,
            family_incidences: self
                .family_incidences
                .iter()
                .map(
                    |((variable_family, constraint_family), incidences)| FamilyIncidenceMetrics {
                        variable_family: variable_family.name().to_string(),
                        constraint_family: constraint_family.name().to_string(),
                        incidences: *incidences,
                    },
                )
                .collect(),
        };

        ModelComplexityMetrics {
            variables: VariableDomainMetrics {
                coverage: MetricCoverage::Complete,
                total_variables: variable_count,
                boolean_variables,
                integer_variables,
                log2_domain_volume,
                by_family: variable_families,
            },
            constraints: Some(constraint_summary),
            factor_graph: Some(factor_graph),
            coupling: Some(CouplingMetrics {
                facility_network_incidences: self.facility_network_incidences,
                shared_network_facility_pairs: self.shared_network_facility_pairs,
                cross_family_constraints: self.cross_family_constraints,
                placement_routing_constraints: self.placement_routing_constraints,
                placement_routing_incidences: self.placement_routing_incidences,
                network_collision_constraints: self.network_collision_constraints,
                objective_incidences: self.objective_incidences,
            }),
            symmetry: None,
            estimated_bytes: None,
        }
    }
}

fn record_aggregate(
    aggregate: &mut ConstraintAggregate,
    arity: u64,
    terms: u64,
    maximum_absolute_coefficient: u64,
) {
    aggregate.constraints += 1;
    aggregate.terms += terms;
    *aggregate.arities.entry(arity).or_default() += 1;
    aggregate.maximum_absolute_coefficient = aggregate
        .maximum_absolute_coefficient
        .max(maximum_absolute_coefficient);
}

fn variable_family_metrics(
    family: VariableFamily,
    mut cardinalities: Vec<u64>,
) -> VariableFamilyMetrics {
    cardinalities.sort_unstable();
    let boolean_variables = cardinalities
        .iter()
        .filter(|cardinality| **cardinality == 2)
        .count() as u64;
    let total_variables = cardinalities.len() as u64;
    VariableFamilyMetrics {
        family: family.name().to_string(),
        total_variables,
        boolean_variables,
        integer_variables: total_variables - boolean_variables,
        domains: DomainCardinalitySummary {
            minimum: cardinalities.first().copied().unwrap_or(0),
            maximum: cardinalities.last().copied().unwrap_or(0),
            p50: percentile(&cardinalities, 50),
            p95: percentile(&cardinalities, 95),
            log2_volume: cardinalities
                .iter()
                .map(|cardinality| (*cardinality as f64).log2())
                .sum(),
        },
    }
}

fn percentile(values: &[u64], percentile: usize) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut values = values.to_vec();
    values.sort_unstable();
    let index = ((values.len() - 1) * percentile).div_ceil(100);
    values[index]
}

fn percentile_histogram(histogram: &BTreeMap<u64, u64>, percentile: u64) -> u64 {
    let total = histogram.values().sum::<u64>();
    if total == 0 {
        return 0;
    }
    let target = total.saturating_mul(percentile).div_ceil(100);
    let mut observed = 0_u64;
    for (value, count) in histogram {
        observed += count;
        if observed >= target {
            return *value;
        }
    }
    0
}

fn maximum_observed(histogram: &BTreeMap<u64, u64>) -> u64 {
    histogram
        .last_key_value()
        .map(|(value, _)| *value)
        .unwrap_or(0)
}

fn expand_histogram(histogram: &BTreeMap<u64, u64>) -> Vec<u64> {
    histogram
        .iter()
        .flat_map(|(value, count)| std::iter::repeat_n(*value, *count as usize))
        .collect()
}

fn mean(values: &[u64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<u64>() as f64 / values.len() as f64
    }
}

fn find(records: &mut BTreeMap<DomainId, VariableRecord>, variable: DomainId) -> DomainId {
    let parent = records[&variable].parent;
    if parent == variable {
        return variable;
    }
    let root = find(records, parent);
    records.get_mut(&variable).expect("record exists").parent = root;
    root
}

fn union(records: &mut BTreeMap<DomainId, VariableRecord>, left: DomainId, right: DomainId) {
    let left_root = find(records, left);
    let right_root = find(records, right);
    if left_root == right_root {
        return;
    }
    let left_rank = records[&left_root].rank;
    let right_rank = records[&right_root].rank;
    if left_rank < right_rank {
        records.get_mut(&left_root).expect("record exists").parent = right_root;
    } else {
        records.get_mut(&right_root).expect("record exists").parent = left_root;
        if left_rank == right_rank {
            records.get_mut(&left_root).expect("record exists").rank += 1;
        }
    }
}

fn connected_components(records: &mut BTreeMap<DomainId, VariableRecord>) -> u64 {
    let variables = records.keys().copied().collect::<Vec<_>>();
    variables
        .into_iter()
        .map(|variable| find(records, variable))
        .collect::<BTreeSet<_>>()
        .len() as u64
}

#[cfg(test)]
mod tests {
    use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
    use pumpkin_solver::core::results::SatisfactionResult;
    use pumpkin_solver::core::termination::Indefinite;
    use pumpkin_solver::core::variables::TransformableVariable;

    use super::*;

    #[test]
    fn records_domains_constraint_degrees_and_family_coupling() {
        let mut model = RecordedModel::default();
        let tag = model.new_constraint_tag();
        let placement = model.new_variable(VariableFamily::Placement, 0, 1, "placement");
        let route = model.new_variable(VariableFamily::RouteCell, 0, 1, "route");
        model.post_less_than_or_equals(
            ConstraintFamily::RouteCellActivation,
            vec![placement.scaled(1), route.scaled(-1)],
            0,
            1,
            tag,
        );

        let metrics = model.metrics();

        assert_eq!(metrics.variables.total_variables, 2);
        assert_eq!(metrics.constraints.as_ref().unwrap().total_constraints, 1);
        assert_eq!(
            metrics.factor_graph.as_ref().unwrap().connected_components,
            Some(1)
        );
        assert_eq!(
            metrics
                .coupling
                .as_ref()
                .unwrap()
                .placement_routing_constraints,
            1
        );
    }

    #[test]
    fn records_native_predicate_clause_without_auxiliary_model_variable() {
        let mut model = RecordedModel::default();
        let tag = model.new_constraint_tag();
        let selected = model.new_variable(VariableFamily::RouteArc, 0, 1, "selected");
        let item = model.new_variable(VariableFamily::ArmItem, 0, 3, "item");
        let variables_before = model.variable_catalog().len();

        model.post_predicate_clause(
            ConstraintFamily::MaterialSeparator,
            &[selected, item],
            vec![
                selected.equality_predicate(0),
                item.disequality_predicate(2),
            ],
            tag,
        );

        assert_eq!(model.variable_catalog().len(), variables_before);
        let metrics = model.metrics();
        assert_eq!(metrics.variables.total_variables, 2);
        let constraints = metrics.constraints.unwrap();
        assert_eq!(constraints.total_constraints, 1);
        assert_eq!(constraints.total_terms, 2);
        assert!(constraints.by_family.iter().any(|family| {
            family.family == "material-separator" && family.constraints == 1 && family.terms == 2
        }));
    }

    #[test]
    fn material_separator_exclusion_clause_preserves_every_state_except_selected_item_flow() {
        for selected in 0..=1 {
            for item in 0..=3 {
                assert_eq!(
                    fixed_material_separator_clause_is_satisfiable(selected, item),
                    selected == 0 || item != 2,
                    "state selected={selected}, item={item}",
                );
            }
        }
    }

    #[test]
    fn tracked_table_accepts_exactly_the_native_table_assignments() {
        let rows = vec![vec![0, 1, 2], vec![1, 0, 3], vec![1, 1, 4]];
        for placement in 0..=1 {
            for port in 0..=1 {
                for geometry in 2..=4 {
                    let native = fixed_table_state_is_satisfiable(
                        rows.clone(),
                        [placement, port, geometry],
                        false,
                    );
                    let tracked = fixed_table_state_is_satisfiable(
                        rows.clone(),
                        [placement, port, geometry],
                        true,
                    );
                    assert_eq!(tracked, native, "state [{placement}, {port}, {geometry}]");
                    assert_eq!(
                        tracked,
                        rows.contains(&vec![placement, port, geometry]),
                        "state [{placement}, {port}, {geometry}]"
                    );
                }
            }
        }
    }

    fn fixed_table_state_is_satisfiable(
        rows: Vec<Vec<i32>>,
        state: [i32; 3],
        tracked: bool,
    ) -> bool {
        let mut model = RecordedModel::default();
        let tag = model.new_constraint_tag();
        let placement = model.new_variable(VariableFamily::Placement, 0, 1, "placement");
        let port = model.new_variable(VariableFamily::Endpoint, 0, 1, "port");
        let geometry = model.new_variable(VariableFamily::EndpointGeometry, 2, 4, "geometry");
        let variables = vec![placement, port, geometry];
        if tracked {
            let selectors = model.post_tracked_table(
                ConstraintFamily::EndpointLink,
                variables.clone(),
                rows,
                tag,
            );
            assert_eq!(selectors.len(), 3);
        } else {
            model.post_table(ConstraintFamily::EndpointLink, variables.clone(), rows, tag);
        }
        for (variable, value) in variables.into_iter().zip(state) {
            model.post_equals(
                ConstraintFamily::ResearchFixation,
                vec![variable.scaled(1)],
                value,
                1,
                tag,
            );
        }

        let mut brancher = model.solver_mut().default_brancher();
        let mut termination = Indefinite;
        let mut resolver = ResolutionResolver::default();
        matches!(
            model
                .solver_mut()
                .satisfy(&mut brancher, &mut termination, &mut resolver),
            SatisfactionResult::Satisfiable(_)
        )
    }

    fn fixed_material_separator_clause_is_satisfiable(
        selected_value: i32,
        item_value: i32,
    ) -> bool {
        let mut model = RecordedModel::default();
        let tag = model.new_constraint_tag();
        let selected = model.new_variable(VariableFamily::RouteArc, 0, 1, "selected");
        let item = model.new_variable(VariableFamily::ArmItem, 0, 3, "item");
        model.post_predicate_clause(
            ConstraintFamily::MaterialSeparator,
            &[selected, item],
            vec![
                selected.equality_predicate(0),
                item.disequality_predicate(2),
            ],
            tag,
        );
        model.post_equals(
            ConstraintFamily::ResearchFixation,
            vec![selected.scaled(1)],
            selected_value,
            1,
            tag,
        );
        model.post_equals(
            ConstraintFamily::ResearchFixation,
            vec![item.scaled(1)],
            item_value,
            1,
            tag,
        );

        let mut brancher = model.solver_mut().default_brancher();
        let mut termination = Indefinite;
        let mut resolver = ResolutionResolver::default();
        matches!(
            model
                .solver_mut()
                .satisfy(&mut brancher, &mut termination, &mut resolver),
            SatisfactionResult::Satisfiable(_)
        )
    }
}
