use super::*;
use crate::full_factory::{
    engine, observation,
    strips::{self, Axis},
};

fn episode(w: Witness, catalogs: std::sync::Arc<FactoryEditCatalogs>) -> engine::Episode {
    engine::Episode::new(
        fixture_request(&w),
        w,
        catalogs,
        engine::Config {
            input_contract: "current-history-16".into(),
            horizon: 32,
            elapsed_ms: 30000,
            route_work: 5000,
            memory: true,
            vocabulary: poses::VocabularyOptions::default(),
        },
    )
    .unwrap()
}

#[test]
fn two_decisions_compact_both_axes_and_validate_the_complete_factory() {
    let (w, catalogs, _) = fixture();
    let mut e = episode(w, std::sync::Arc::new(catalogs));
    for axis in [Axis::X, Axis::Y] {
        let before = e.session.snapshot().current.clone();
        let count = strips::candidates(&before, axis).unwrap().remove.len() as i64;
        assert!(count > 1, "fixture must exercise a genuinely bulk edit");
        let prepared = e.prepare(true, true).unwrap();
        let domain = e.diagnostic_domain().unwrap();
        let index = domain["actions"]
            .as_array()
            .unwrap()
            .iter()
            .position(|a| {
                a["kind"] == "contract" && a["axis"] == serde_json::to_value(axis).unwrap()
            })
            .unwrap();
        let result = e.step(prepared.handle, index).unwrap();
        assert_eq!(result["status"], "improved", "{result}");
        let after = e.session.snapshot().current;
        assert!(strips::candidates(after, axis).unwrap().remove.is_empty());
        let b = extent(&before).unwrap();
        let a = extent(after).unwrap();
        match axis {
            Axis::X => assert_eq!((a.width(), a.height()), (b.width() - count, b.height())),
            Axis::Y => assert_eq!((a.width(), a.height()), (b.width(), b.height() - count)),
        }
        assert_eq!(after.placements.len(), before.placements.len());
        assert_eq!(
            after.logistics_components.len(),
            before.logistics_components.len()
        );
    }
    assert_eq!(e.selected, 2);
    assert_eq!(e.session.snapshot().proposals, 2);
}

#[test]
fn corridor_insertion_is_atomic_and_axis_contraction_is_its_geometric_inverse() {
    let (w, catalogs, _) = fixture();
    let catalogs = std::sync::Arc::new(catalogs);
    for axis in [Axis::X, Axis::Y] {
        let mut e = episode(w.clone(), catalogs.clone());
        let count = strips::candidates(e.session.snapshot().current, axis)
            .unwrap()
            .remove
            .len() as i64;
        let compact = strips::edit(e.session.snapshot().current, axis, None, count).unwrap();
        // A native episode constructor validates the full request before the probe.
        e = episode(compact, catalogs.clone());
        let before = e.session.snapshot().current.clone();
        let prepared = e.prepare(true, true).unwrap();
        let domain = e.diagnostic_domain().unwrap();
        let index = domain["actions"]
            .as_array()
            .unwrap()
            .iter()
            .position(|a| {
                a["kind"] == "insert"
                    && a["axis"] == serde_json::to_value(axis).unwrap()
                    && a["count"].as_i64().unwrap() > 1
            })
            .unwrap();
        let count = domain["actions"][index]["count"].as_i64().unwrap();
        let result = e.step(prepared.handle, index).unwrap();
        assert_eq!(result["status"], "accepted_exploration", "{result}");
        let expanded = e.session.snapshot().current;
        assert_eq!(
            strips::candidates(expanded, axis).unwrap().remove.len() as i64,
            count
        );
        let contracted = strips::edit(expanded, axis, None, count).unwrap();
        // Physical identity ignores serialization ordering, not geometry or rates.
        assert_eq!(
            crate::full_factory::identity::Identity::new(&before).physical,
            crate::full_factory::identity::Identity::new(&contracted).physical
        );
        assert_eq!(e.selected, 1);
        assert!(strips::edit(&before, axis, Some(i64::MAX), count).is_err());
        assert!(strips::edit(expanded, axis, None, count + 1).is_err());
    }
}

#[test]
fn compact_candidates_keep_fixed_features_and_include_distant_component_rotation() {
    let (w, catalogs, _) = fixture();
    let catalogs = std::sync::Arc::new(catalogs);
    let e = episode(w, catalogs.clone());
    let w = e.session.snapshot().current;
    let domain = candidates::describe(w, &catalogs, &poses::VocabularyOptions::default()).unwrap();
    let world = observation::world(w, &domain.ports).unwrap();
    let encoded = observation::candidates(w, &domain, &world);
    let mut contracts = 0;
    let mut inserts = 0;
    for (i, action) in domain.actions.iter().enumerate() {
        assert_eq!(encoded.actions[i].len(), 32);
        assert_eq!(encoded.token(i).len(), 42);
        match action {
            candidates::Action::Contract { count, .. } => {
                contracts += 1;
                assert!(*count > 1);
                assert!(encoded.actions[i][30] > 0.);
                assert_eq!(encoded.actions[i][31], 1.);
            }
            candidates::Action::Insert { .. } => {
                inserts += 1;
                assert!(encoded.actions[i][30] > 0.);
                assert_eq!(encoded.actions[i][31], 0.);
            }
            _ => {}
        }
    }
    assert_eq!(contracts, 2);
    assert!(inserts <= 128);
    for c in &w.logistics_components {
        let choices: Vec<_> = domain.actions.iter().filter(|a|
            matches!(a, candidates::Action::Component { component, .. } if component == &c.id)).collect();
        assert!(!choices.is_empty() && choices.len() <= 32);
        assert!(choices.iter().any(
            |a| matches!(a, candidates::Action::Component { dx, dy, rotation, .. }
            if dx.abs() + dy.abs() > 2 && *rotation != c.rotation)
        ));
    }
}

#[test]
fn a_distant_component_move_and_rotation_can_validate_as_one_edit() {
    use aic_data::layouts::FactoryEditStatus;
    let (w, catalogs, _) = fixture();
    let catalogs = std::sync::Arc::new(catalogs);
    let mut e = episode(w, catalogs.clone());
    let before = e.session.snapshot().current.clone();
    let domain =
        candidates::describe(&before, &catalogs, &poses::VocabularyOptions::default()).unwrap();
    for action in &domain.actions {
        let candidates::Action::Component {
            component,
            dx,
            dy,
            rotation,
            ..
        } = action
        else {
            continue;
        };
        let old = before
            .logistics_components
            .iter()
            .find(|c| &c.id == component)
            .unwrap();
        if dx.abs() + dy.abs() <= 2 || *rotation == old.rotation {
            continue;
        }
        if let Ok(execution) = candidates::execute(&before, &catalogs, action, 5000) {
            if let Some(witness) = execution.witness {
                let accepted = e.session.propose(&witness);
                if matches!(
                    accepted.status,
                    FactoryEditStatus::Improved
                        | FactoryEditStatus::AcceptedExploration
                        | FactoryEditStatus::AcceptedEqual
                ) {
                    return;
                }
            }
        }
    }
    panic!("No distant translated/rotated component candidate validated on the crossing fixture");
}
