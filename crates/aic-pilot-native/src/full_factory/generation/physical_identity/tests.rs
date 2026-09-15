use super::*;
use crate::full_factory::tests::fixture;

fn translate(w: &mut WitnessProbeInput, dx: i64, dy: i64) {
    for p in &mut w.placements {
        p.x += dx;
        p.y += dy;
    }
    for n in &mut w.transport_networks {
        n.cells
            .iter_mut()
            .chain(n.segments.iter_mut().flat_map(|s| [&mut s.from, &mut s.to]))
            .chain(n.terminals.iter_mut().map(|t| &mut t.position))
            .for_each(|p| {
                p.x += dx;
                p.y += dy;
            });
    }
    for c in &mut w.logistics_components {
        c.position.x += dx;
        c.position.y += dy;
    }
}

#[test]
fn translated_physical_identity_preserves_rates_but_distinguishes_incidence() {
    let (w, c, _) = fixture();
    let limits = Limits {
        max_individualization_nodes: 100_000,
    };
    let key = |w: &WitnessProbeInput| identify(w, &c, &limits, "crossing").unwrap().0;
    let expected = key(&w);
    for (dx, dy) in [(123, -799), (-100_000, 12345)] {
        let mut changed = w.clone();
        translate(&mut changed, dx, dy);
        assert_eq!(key(&changed), expected);
    }
    let max_x = w
        .placements
        .iter()
        .map(|p| p.x + p.width - 1)
        .chain(
            w.transport_networks
                .iter()
                .flat_map(|n| n.cells.iter().map(|p| p.x)),
        )
        .chain(w.logistics_components.iter().map(|c| c.position.x))
        .max()
        .unwrap();
    let mut edge = w.clone();
    translate(&mut edge, i64::MAX - max_x, 0);
    assert_eq!(
        validate_and_identify(
            &crate::full_factory::tests::fixture_request(&w),
            &edge,
            &c,
            &limits,
            "extreme-origin"
        )
        .unwrap(),
        expected
    );
    let mut rate_only = w.clone();
    rate_only.transport_networks[0].segments[0].rate.numerator *= 2;
    assert_eq!(key(&rate_only).structural, expected.structural);
    assert_ne!(key(&rate_only).exact_problem, expected.exact_problem);
    let mut fraction = w.clone();
    for n in &mut fraction.transport_networks {
        for s in &mut n.segments {
            s.rate.numerator *= 2;
            s.rate.denominator *= 2;
        }
    }
    assert_eq!(key(&fraction), expected);
    let mut reversed = w.clone();
    let segment = &mut reversed.transport_networks[0].segments[0];
    std::mem::swap(&mut segment.from, &mut segment.to);
    assert_ne!(key(&reversed).structural, expected.structural);
    let mut endpoint = w.clone();
    let terminals = &mut endpoint.transport_networks[0].terminals;
    terminals[0].endpoint = terminals[1].endpoint.clone();
    // Copy a facility endpoint and direction together, keeping its referenced recipe commodity valid.
    terminals[0].direction = terminals[1].direction;
    assert_ne!(key(&endpoint).structural, expected.structural);
    let mut commodity = w.clone();
    let item = commodity.transport_networks[0].item.clone();
    // Commodity labels are equality partitions: a new label alone changes no physical spelling.
    for n in &mut commodity.transport_networks {
        if n.item == item {
            n.item = "renamed-commodity".into();
        }
    }
    // Catalog renaming is exercised by the admitted full-namespace fixture test; missing
    // recipe references are rejected here instead of being silently erased.
    assert!(identify(&commodity, &c, &limits, "missing-commodity").is_err());
    let mut channels = w.clone();
    let owned = channels
        .transport_networks
        .iter()
        .position(|n| !n.component_ids.is_empty())
        .unwrap();
    channels.transport_networks[owned].component_ids.clear();
    assert_ne!(key(&channels).structural, expected.structural);
    let error = identify(
        &w,
        &c,
        &Limits {
            max_individualization_nodes: 1,
        },
        "cases/crossing.json",
    )
    .unwrap_err();
    let error: serde_json::Value = serde_json::from_str(&error).unwrap();
    assert_eq!(error["code"], "aic-learning.physical-identity-budget");
    assert_eq!(error["entity"], "cases/crossing.json");
    assert_eq!(error["work_used"], 1);
}

#[test]
#[ignore = "requires AIC_IDENTITY_STUDY bounded retained-input manifest and AIC_IDENTITY_REPORT"]
fn retained_admission_identity_work_study() {
    use serde_json::{Value, json};
    use std::{path::Path, time::Instant};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config: Value =
        crate::input::read(&root, Path::new("tools/native-training/training.json")).unwrap();
    let catalogs = crate::input::load_catalogs(
        &root,
        &serde_json::from_value(config["catalogs"].clone()).unwrap(),
    )
    .unwrap();
    let limits: Limits =
        serde_json::from_value(config["generation"]["identity_limits"].clone()).unwrap();
    let manifest: Vec<Value> = crate::input::read(
        Path::new("."),
        Path::new(&std::env::var("AIC_IDENTITY_STUDY").unwrap()),
    )
    .unwrap();
    assert!(!manifest.is_empty() && manifest.len() <= 32);
    let mut rows = vec![];
    for case in manifest {
        let began = Instant::now();
        let path = Path::new(case["input"].as_str().unwrap());
        let bytes = std::fs::read(path).unwrap();
        assert!(bytes.len() <= 16 * 1024 * 1024);
        let source_sha256 = crate::input::hash(&bytes);
        let input: crate::generation::source::Generated = serde_json::from_slice(&bytes).unwrap();
        let read_seconds = began.elapsed().as_secs_f64();
        let start = Instant::now();
        let valid = aic_data::layouts::validate_factory_layout(
            &input.request,
            &input.original,
            &catalogs.data,
        );
        assert!(
            valid.factory_validated,
            "{}: {:?}",
            path.display(),
            valid.diagnostics
        );
        let validation_seconds = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let graph = graph(&input.original, &catalogs.data).unwrap();
        let graph_seconds = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let (keys, work) =
            aic_layout::physical_identity::identities(&graph, &limits, path.to_str().unwrap())
                .unwrap();
        let canonical_seconds = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let encoded = serde_json::to_vec(&keys).unwrap();
        let serialization_seconds = start.elapsed().as_secs_f64();
        rows.push(json!({"id":case["id"],"input":path,"source_sha256":source_sha256,"input_bytes":bytes.len(),
            "facilities":input.original.placements.len(),"vertices":graph.vertices.len(),"relations":graph.relations.len(),
            "work":work,"read_seconds":read_seconds,"validation_seconds":validation_seconds,"graph_seconds":graph_seconds,
            "canonical_seconds":canonical_seconds,"serialization_seconds":serialization_seconds,"identity_bytes":encoded.len(),
            "total_seconds":began.elapsed().as_secs_f64(),"identity":keys}));
    }
    let report = json!({"schema":"aic-physical-identity-study-v1","catalog_sha256":catalogs.identity,"limits":limits,"rows":rows});
    std::fs::write(
        std::env::var("AIC_IDENTITY_REPORT").unwrap(),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
