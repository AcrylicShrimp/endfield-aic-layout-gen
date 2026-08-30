use std::collections::BTreeSet;
use std::fmt::Write;

use sha2::{Digest, Sha256};

use crate::logistics::{LogisticsComponentKind, TransportKind};

pub(super) fn logistics_component_id(
    kind: LogisticsComponentKind,
    transport: TransportKind,
    x: i64,
    y: i64,
    owners: &BTreeSet<String>,
) -> String {
    let mut digest = Sha256::new();
    hash_text(
        &mut digest,
        match kind {
            LogisticsComponentKind::Splitter => "splitter",
            LogisticsComponentKind::Converger => "converger",
            LogisticsComponentKind::Bridge => "bridge",
        },
    );
    hash_text(
        &mut digest,
        match transport {
            TransportKind::Belt => "belt",
            TransportKind::Pipe => "pipe",
        },
    );
    digest.update(x.to_be_bytes());
    digest.update(y.to_be_bytes());
    for owner in owners {
        hash_text(&mut digest, owner);
    }
    format!("component:{}", hex_digest(digest))
}

fn hash_text(digest: &mut Sha256, text: &str) {
    digest.update((text.len() as u64).to_be_bytes());
    digest.update(text.as_bytes());
}

fn hex_digest(digest: Sha256) -> String {
    digest
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        })
}
