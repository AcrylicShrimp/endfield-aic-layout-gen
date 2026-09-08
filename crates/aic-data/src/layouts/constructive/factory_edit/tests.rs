use std::collections::BTreeMap;

use super::*;
use crate::layouts::{TransportNetworkEndpoint, TransportNetworkSegment, WorldGridPosition};
use crate::logistics::LogisticsComponentKind;

struct Fixture {
    catalogs: Arc<FactoryEditCatalogs>,
    request: FactoryEditRequest,
    input: WitnessProbeInput,
}

impl Fixture {
    fn load() -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let data = root.join("data/game/normalized");
        let input: WitnessProbeInput = serde_json::from_slice(
            &std::fs::read(root.join(
                "docs/benchmarks/constructive-bound-interconnect/fixtures/crossing-witness.json",
            ))
            .unwrap(),
        )
        .unwrap();
        let catalogs = Arc::new(FactoryEditCatalogs {
            facilities: ValidatedFacilityCatalog::try_from_catalog(
                crate::facilities::load_facility_catalog(data.join("facilities.json")).unwrap(),
            )
            .unwrap(),
            recipes: ValidatedRecipeBook::try_from_recipe_book(
                crate::recipes::load_recipe_book(data.join("recipes.json")).unwrap(),
            )
            .unwrap(),
            items: ValidatedItemCatalog::try_from_catalog(
                crate::logistics::load_item_catalog(data.join("items.json")).unwrap(),
            )
            .unwrap(),
            transports: ValidatedTransportCatalog::try_from_catalog(
                crate::logistics::load_transport_catalog(data.join("transports.json")).unwrap(),
            )
            .unwrap(),
            components: ValidatedLogisticsComponentCatalog::try_from_catalog(
                crate::logistics::load_logistics_component_catalog(
                    data.join("logistics-components.json"),
                )
                .unwrap(),
            )
            .unwrap(),
        });
        // This immutable external fixture defines the test obligation. Mutations never
        // regenerate the request. The end-to-end importer instead reads original graphs.
        let instances = input
            .placements
            .iter()
            .map(|p| FactoryEditInstance {
                instance: p.instance.clone(),
                recipe: p.recipe.clone(),
                utilization: input.utilizations[&p.instance],
            })
            .collect();
        let mut terminals = BTreeMap::new();
        for n in &input.transport_networks {
            for t in &n.terminals {
                let endpoint = match &t.endpoint {
                    TransportNetworkEndpoint::Facility { instance, .. } => {
                        FactoryEditEndpoint::Facility {
                            instance: instance.clone(),
                        }
                    }
                    TransportNetworkEndpoint::External { node, .. } => {
                        FactoryEditEndpoint::External { node: node.clone() }
                    }
                };
                let rate = terminals
                    .entry((endpoint, t.direction, n.item.clone()))
                    .or_insert(Rate::zero());
                *rate = rate.checked_add(t.rate).unwrap();
            }
        }
        let request = FactoryEditRequest {
            schema_version: 1,
            instances,
            terminals: terminals
                .into_iter()
                .map(|((endpoint, direction, item), rate)| FactoryEditTerminal {
                    endpoint,
                    direction,
                    item,
                    rate,
                })
                .collect(),
        };
        Self {
            catalogs,
            request,
            input,
        }
    }

    fn check(&self, input: &WitnessProbeInput) -> FactoryLayoutAcceptance {
        validate_factory_layout(&self.request, input, &self.catalogs)
    }

    fn session(&self, input: WitnessProbeInput, proposals: u64) -> FactoryEditSession {
        FactoryEditSession::new(
            self.request.clone(),
            input,
            Arc::clone(&self.catalogs),
            FactoryEditBudget {
                proposals,
                elapsed_ms: 30000,
            },
        )
        .unwrap()
    }
}

fn translate(w: &mut WitnessProbeInput, dx: i64, dy: i64) {
    let move_point = |p: &mut WorldGridPosition| {
        p.x += dx;
        p.y += dy;
    };
    for p in &mut w.placements {
        p.x += dx;
        p.y += dy;
    }
    for n in &mut w.transport_networks {
        for p in &mut n.cells {
            move_point(p);
        }
        for s in &mut n.segments {
            move_point(&mut s.from);
            move_point(&mut s.to);
        }
        for t in &mut n.terminals {
            move_point(&mut t.position);
        }
    }
    for c in &mut w.logistics_components {
        move_point(&mut c.position);
    }
}

// A supplied physical proposal that translates downstream bodies and explicitly
// lengthens every crossing route. The production executor does not choose this edit.
fn stretch(w: &WitnessProbeInput, column: i64) -> WitnessProbeInput {
    let mut result = w.clone();
    let shift = |p: &WorldGridPosition| WorldGridPosition {
        x: p.x + i64::from(p.x >= column),
        y: p.y,
    };
    for p in &mut result.placements {
        if p.x >= column {
            p.x += 1;
        }
    }
    for c in &mut result.logistics_components {
        c.position = shift(&c.position);
    }
    for n in &mut result.transport_networks {
        n.cells = n.cells.iter().map(shift).collect();
        let mut segments = Vec::new();
        for s in &n.segments {
            let mut a = shift(&s.from);
            let b = shift(&s.to);
            while (a.x - b.x).abs() + (a.y - b.y).abs() > 1 {
                let next = WorldGridPosition {
                    x: a.x + (b.x - a.x).signum(),
                    y: a.y + (b.y - a.y).signum(),
                };
                n.cells.push(next.clone());
                segments.push(TransportNetworkSegment {
                    from: a,
                    to: next.clone(),
                    rate: s.rate,
                });
                a = next;
            }
            segments.push(TransportNetworkSegment {
                from: a,
                to: b,
                rate: s.rate,
            });
        }
        n.segments = segments;
        for t in &mut n.terminals {
            t.position = shift(&t.position);
        }
    }
    result
}

#[test]
fn full_shared_graph_accepts_and_translation_does_not_manufacture_area_gain() {
    let f = Fixture::load();
    let report = f.check(&f.input);
    assert!(report.factory_validated, "{:?}", report.diagnostics);
    let metrics = report.metrics.unwrap();
    assert_eq!((metrics.width, metrics.height), (35, 20));
    assert_eq!(metrics.score[0], 700);
    assert_eq!(metrics.empty_xy_cells + metrics.occupied_xy_cells, 700);
    assert_eq!(
        metrics.facility_xy_cells + metrics.transport_xy_cells,
        metrics.occupied_xy_cells
    );
    // A same-layer bridge costs one physical tile even though two networks use it.
    let network_cells: usize = f
        .input
        .transport_networks
        .iter()
        .map(|n| n.cells.len())
        .sum();
    assert!(metrics.score[1] < network_cells as u64);
    for (dx, dy) in [(100, 200), (-100, -200), (i64::MIN, i64::MIN)] {
        let mut shifted = f.input.clone();
        translate(&mut shifted, dx, dy);
        assert_eq!(f.check(&shifted).metrics.unwrap(), metrics);
    }
}

#[test]
fn shared_component_rules_remain_the_production_validators_rules() {
    let f = Fixture::load();
    for kind in [
        LogisticsComponentKind::Splitter,
        LogisticsComponentKind::Converger,
        LogisticsComponentKind::Bridge,
    ] {
        let mut w = f.input.clone();
        w.logistics_components.retain(|c| c.kind != kind);
        for n in &mut w.transport_networks {
            n.component_ids
                .retain(|id| w.logistics_components.iter().any(|c| c.id == *id));
        }
        let r = f.check(&w);
        assert!(!r.factory_validated, "{kind:?}");
        assert_eq!(r.diagnostics[0].code, "factory.geometry-invalid");
    }
}

#[test]
fn original_external_identity_is_checked_beyond_geometry_and_recipe_completeness() {
    let f = Fixture::load();
    let mut w = f.input.clone();
    let t = w
        .transport_networks
        .iter_mut()
        .flat_map(|n| &mut n.terminals)
        .find(|t| matches!(t.endpoint, TransportNetworkEndpoint::External { .. }))
        .unwrap();
    t.node = "invented-procurement".into();
    let TransportNetworkEndpoint::External { node, .. } = &mut t.endpoint else {
        unreachable!()
    };
    *node = t.node.clone();
    let probe = super::super::probe_constructive_witness(
        &w,
        &f.catalogs.facilities,
        &f.catalogs.recipes,
        &f.catalogs.items,
        &f.catalogs.transports,
        &f.catalogs.components,
    );
    assert!(probe.covered_geometry_validated && probe.recipe_coverage_complete);
    let report = f.check(&w);
    assert!(!report.factory_validated);
    assert_eq!(report.diagnostics[0].code, "factory.original-terminals");
}

#[test]
fn missing_routes_or_changed_inventory_cannot_be_accepted_as_a_complete_factory() {
    let f = Fixture::load();
    let mut w = f.input.clone();
    w.transport_networks.clear();
    w.logistics_components.clear();
    assert_eq!(f.check(&w).diagnostics[0].code, "factory.recipe-coverage");
    w = f.input.clone();
    w.placements.pop();
    assert_eq!(f.check(&w).diagnostics[0].code, "factory.inventory");
    w = f.input.clone();
    w.placements[0].recipe = "different-recipe".into();
    assert_eq!(f.check(&w).diagnostics[0].code, "factory.inventory");
    w = f.input.clone();
    w.utilizations.values_mut().next().unwrap().denominator = 0;
    assert_eq!(f.check(&w).diagnostics[0].code, "factory.inventory");
}

#[test]
fn malformed_original_request_is_rejected_before_root_acceptance() {
    let f = Fixture::load();
    let mut request = f.request.clone();
    request.terminals.pop();
    assert!(!validate_factory_layout(&request, &f.input, &f.catalogs).factory_validated);
    request = f.request.clone();
    request.instances[0].utilization.denominator = 0;
    assert_eq!(
        validate_factory_layout(&request, &f.input, &f.catalogs).diagnostics[0].code,
        "factory.request-instance"
    );
    request = f.request.clone();
    request.terminals.push(request.terminals[0].clone());
    assert_eq!(
        validate_factory_layout(&request, &f.input, &f.catalogs).diagnostics[0].code,
        "factory.request-terminal"
    );
}

#[test]
fn relocation_and_explicit_reconnection_improve_while_worse_and_invalid_proposals_preserve_best() {
    let f = Fixture::load();
    let expanded = stretch(&f.input, 6);
    assert!(
        expanded
            .placements
            .iter()
            .zip(&f.input.placements)
            .any(|(a, b)| a.x != b.x)
    );
    assert!(f.check(&expanded).factory_validated);
    let mut session = f.session(expanded.clone(), 3);
    let first = session.propose(&f.input);
    assert_eq!(first.status, FactoryEditStatus::Improved);
    assert!(first.best.score < session.initial_metrics.score);
    let best = serde_json::to_value(session.snapshot().best).unwrap();
    let second = session.propose(&expanded);
    assert_eq!(second.status, FactoryEditStatus::AcceptedExploration);
    assert!(second.current.score > second.best.score);
    let current = serde_json::to_value(session.snapshot().current).unwrap();
    let mut invalid = f.input.clone();
    invalid.placements[0].x += 1;
    assert_eq!(
        session.propose(&invalid).status,
        FactoryEditStatus::Rejected
    );
    assert_eq!(serde_json::to_value(session.snapshot().best).unwrap(), best);
    assert_eq!(
        serde_json::to_value(session.snapshot().current).unwrap(),
        current
    );
    let exhausted = session.propose(&f.input);
    assert_eq!(exhausted.status, FactoryEditStatus::BudgetExhausted);
    assert!(!exhausted.charged);
    assert_eq!(session.snapshot().proposals, 3);
}

#[test]
fn deadline_after_validation_prevents_commit_without_reclassifying_validity() {
    let f = Fixture::load();
    let mut session = f.session(stretch(&f.input, 6), 2);
    let initial = session.current_metrics.clone();
    let validated = validation::accept(&f.request, &f.input, &f.catalogs);
    session.proposals = 1;
    // Simulate the clock crossing the deadline during independent validation.
    session.started = Instant::now() - Duration::from_millis(session.budget.elapsed_ms + 1);
    let result = session.finish(validated, 1.0);
    assert_eq!(result.status, FactoryEditStatus::Late);
    assert!(result.charged && result.candidate_validated);
    assert_eq!(result.current, initial);
    assert_eq!(result.best, initial);
    assert_eq!(
        session.propose(&f.input).status,
        FactoryEditStatus::BudgetExhausted
    );
}

#[test]
fn malformed_json_and_equal_proposals_both_consume_selected_work() {
    let f = Fixture::load();
    let mut session = f.session(f.input.clone(), 2);
    let before = serde_json::to_value(session.snapshot().best).unwrap();
    let rejected = session.propose_json("not-json");
    assert!(rejected.charged);
    assert_eq!(rejected.proposal, 1);
    assert_eq!(rejected.diagnostics[0].code, "factory.proposal-schema");
    assert_eq!(
        serde_json::to_value(session.snapshot().best).unwrap(),
        before
    );
    let equal = session.propose_json(&serde_json::to_string(&f.input).unwrap());
    assert_eq!(equal.status, FactoryEditStatus::AcceptedEqual);
    assert!(equal.charged && equal.candidate_validated);
    assert!(session.done());
}

#[test]
fn external_terminal_arms_contribute_to_the_last_score_priority() {
    let f = Fixture::load();
    let mut bent = f.input.clone();
    let terminal = bent
        .transport_networks
        .iter_mut()
        .flat_map(|n| &mut n.terminals)
        .find(|t| {
            t.position.x == 0
                && t.position.y == 2
                && matches!(t.endpoint, TransportNetworkEndpoint::External { .. })
        })
        .unwrap();
    let TransportNetworkEndpoint::External { side, .. } = &mut terminal.endpoint else {
        unreachable!()
    };
    *side = crate::facilities::FacilityPortEdge::North;
    let original = f.check(&f.input).metrics.unwrap();
    let changed = f.check(&bent);
    assert!(changed.factory_validated, "{:?}", changed.diagnostics);
    let changed = changed.metrics.unwrap();
    assert_eq!(
        changed.score,
        [original.score[0], original.score[1], original.score[2] + 1]
    );
    let mut session = f.session(bent, 1);
    assert_eq!(
        session.propose(&f.input).status,
        FactoryEditStatus::Improved
    );
}
