use aic_layout::{
    facilities::{FacilityCatalog, ValidatedFacilityCatalog},
    geometry::Bounds,
    logistics::{
        LogisticsComponentCatalog, TransportKind, ValidatedLogisticsComponentCatalog,
        ValidatedTransportCatalog,
    },
    physical::{Block, Connection, Problem},
    rate::Rate,
    validation::Catalogs,
};
use aic_pilot_native::constructive::{
    Limits, Session,
    learning::{Config as DomainConfig, Observation},
};
use serde::de::DeserializeOwned;
use std::{fs::File, path::Path, sync::Arc};

fn read<T: DeserializeOwned>(root: &Path, name: &str) -> T {
    serde_json::from_reader(File::open(root.join(name)).unwrap()).unwrap()
}

fn catalogs() -> Arc<Catalogs> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized");
    Arc::new(Catalogs {
        identity: "1".repeat(64),
        facilities: ValidatedFacilityCatalog::try_from_catalog(read::<FacilityCatalog>(
            &root,
            "facilities.json",
        ))
        .unwrap(),
        transports: ValidatedTransportCatalog::try_from_catalog(read(&root, "transports.json"))
            .unwrap(),
        components: ValidatedLogisticsComponentCatalog::try_from_catalog(read::<
            LogisticsComponentCatalog,
        >(
            &root,
            "logistics-components.json",
        ))
        .unwrap(),
    })
}

pub(crate) fn session(max_turns: u64) -> Session {
    let problem = Problem {
        blocks: ["source", "target"]
            .into_iter()
            .map(|id| Block {
                id: id.into(),
                facility: "grinder-1-mode-normal".into(),
            })
            .collect(),
        terminals: vec![],
        connections: vec![Connection {
            id: "connection-0".into(),
            source: "source".into(),
            target: "target".into(),
            commodity: "test-item".into(),
            transport: TransportKind::Belt,
            rate: Rate {
                numerator: 1,
                denominator: 2,
            },
        }],
    };
    Session::new(
        problem,
        catalogs(),
        Limits {
            max_turns,
            max_extension: 8,
        },
    )
    .unwrap()
}

pub(crate) fn domain() -> DomainConfig {
    DomainConfig {
        placement: Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 9,
            max_y: 9,
        },
        extension_lengths: vec![1, 2, 4],
    }
}

pub(crate) fn observation() -> Observation {
    session(32).learning_observation(&domain()).unwrap()
}
