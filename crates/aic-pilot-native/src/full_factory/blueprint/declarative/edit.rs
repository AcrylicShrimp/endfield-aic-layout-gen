//! Joint port decisions and rigid grid-band transforms preserve every entity.

use indexmap::IndexSet;

use super::*;

impl Session {
    fn facility_nodes(&self, instance: &str) -> Vec<usize> {
        self.blueprint
            .nodes
            .iter()
            .filter_map(|n| match n.terminal.as_ref().map(|t| &t.endpoint) {
                Some(TransportNetworkEndpoint::Facility {
                    instance: owner, ..
                }) if owner == instance => Some(n.id),
                _ => None,
            })
            .collect()
    }

    fn check_entities(&self, state: &State) -> Result<()> {
        if state.geometry.placements.len() != self.blueprint.facilities.len()
            || state.geometry.paths.len() != self.blueprint.edges.len()
            || state.ports.len() != self.blueprint.nodes.len()
            || state.anchors.len() != self.blueprint.nodes.len()
            || state.components.len() != self.blueprint.components.len()
        {
            return Err(error(
                "state-shape",
                "State must retain every original facility, node, edge and component slot",
                "state",
            ));
        }
        frame::from_state(&self.blueprint, state, &self.catalogs.data)?;
        let mut used = IndexSet::new();
        for (p, original) in state
            .geometry
            .placements
            .iter()
            .zip(&self.blueprint.facilities)
        {
            if (&p.instance, &p.facility, &p.recipe)
                != (&original.instance, &original.facility, &original.recipe)
            {
                return Err(error(
                    "facility-identity",
                    "Facility ID, definition and recipe are immutable",
                    p.instance.clone(),
                ));
            }
            let definition = self.catalogs.data.facilities.facility(&p.facility).unwrap();
            if !definition.allowed_rotations.contains(&p.rotation) {
                return Err(error(
                    "pose",
                    "Position or rotation exceeds the runtime catalog/coordinate domain",
                    p.instance.clone(),
                ));
            }
            let (w, h, ports) = catalog::rotate(definition, p.rotation, Point { x: p.x, y: p.y });
            if p.width != w || p.height != h {
                return Err(error(
                    "pose",
                    "Body dimensions or extent disagree with the catalog/coordinate guard",
                    p.instance.clone(),
                ));
            }
            for id in self.facility_nodes(&p.instance) {
                let node = &self.blueprint.nodes[id];
                let terminal = node.terminal.as_ref().unwrap();
                let selected = state.ports[id].as_ref().ok_or_else(|| {
                    error(
                        "port-assignment",
                        "Missing original terminal's physical port",
                        id,
                    )
                })?;
                let port = ports
                    .iter()
                    .find(|port| &port.id == selected)
                    .ok_or_else(|| {
                        error(
                            "port-assignment",
                            "Selected port is absent from this facility's catalog",
                            json!({"node":id,"port":selected}),
                        )
                    })?;
                if port.direction != terminal.direction
                    || json!(port.transport) != self.blueprint.networks[node.network]["transport"]
                {
                    return Err(error(
                        "port-incompatible",
                        "Selected port has the wrong direction or transport",
                        id,
                    ));
                }
                if !used.insert((p.instance.clone(), selected.clone())) {
                    return Err(error(
                        "port-reused",
                        "Distinct original terminals cannot share one physical port",
                        id,
                    ));
                }
            }
        }
        for node in &self.blueprint.nodes {
            let facility = matches!(
                node.terminal.as_ref().map(|t| &t.endpoint),
                Some(TransportNetworkEndpoint::Facility { .. })
            );
            if !facility && state.ports[node.id].is_some() {
                return Err(error(
                    "port-assignment",
                    "Only original facility terminals may select a port",
                    node.id,
                ));
            }
            for component in &node.components {
                let i = self
                    .blueprint
                    .components
                    .iter()
                    .position(|c| &c.id == component)
                    .unwrap();
                if state.anchors[node.id] != Point::from(&state.components[i]) {
                    return Err(error(
                        "component-anchor",
                        "All original component channel anchors must remain attached",
                        node.id,
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_state(&self, state: &State) -> Result<()> {
        self.check_entities(state)?;
        crate::physical_editor::validate_draft(&self.common_problem, &common::draft(state))
            .map_err(|failure| error("common-state", failure.message, failure.entity))
    }

    pub(super) fn edit(
        &self,
        draft: &mut aic_layout::editor::PhysicalDraftV1,
        action: Edit,
    ) -> Result<()> {
        let action = match action {
            Edit::Place {
                instance,
                x,
                y,
                rotation,
                ports,
            } => crate::physical_editor::PhysicalEdit::Place {
                block: instance,
                position: Point { x, y },
                rotation,
                ports: ports
                    .into_iter()
                    .map(|choice| crate::physical_editor::PortSelection {
                        node: choice.node,
                        port: choice.port,
                    })
                    .collect(),
            },
            Edit::InsertBand { axis, at, count } => {
                crate::physical_editor::PhysicalEdit::InsertBand {
                    axis: match axis {
                        Axis::X => crate::physical_editor::Axis::X,
                        Axis::Y => crate::physical_editor::Axis::Y,
                    },
                    at,
                    count,
                }
            }
            Edit::DeleteBand { axis, at, count } => {
                crate::physical_editor::PhysicalEdit::DeleteBand {
                    axis: match axis {
                        Axis::X => crate::physical_editor::Axis::X,
                        Axis::Y => crate::physical_editor::Axis::Y,
                    },
                    at,
                    count,
                }
            }
        };
        crate::physical_editor::apply_edit_in_place(&self.common_problem, draft, action).map_err(
            |failure| {
                let code = failure
                    .code
                    .strip_prefix("physical-editor.")
                    .unwrap_or("common-edit");
                error(code, failure.message, failure.entity)
            },
        )?;
        Ok(())
    }
}
