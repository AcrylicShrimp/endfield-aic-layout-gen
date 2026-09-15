use aic_data::layouts::{FacilityPlacement, FactoryEditCatalogs};
use indexmap::IndexMap;

use super::types::*;

pub use aic_layout::projection::{Port, edge_index, rotate};

pub fn placement_ports(p: &FacilityPlacement, catalogs: &FactoryEditCatalogs) -> Result<Vec<Port>> {
    let spec = catalogs.facilities.facility(&p.facility).ok_or_else(|| {
        Diagnostic::new(
            "factory-unknown-facility",
            "Facility is absent from the runtime catalog",
            p.facility.clone(),
        )
    })?;
    Ok(rotate(spec, p.rotation, Point { x: p.x, y: p.y }).2)
}
pub fn project(w: &Witness, catalogs: &FactoryEditCatalogs) -> Result<IndexMap<String, Vec<Port>>> {
    w.placements
        .iter()
        .map(|p| Ok((p.instance.clone(), placement_ports(p, catalogs)?)))
        .collect()
}
