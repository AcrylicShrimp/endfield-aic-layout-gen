//! Joint port decisions and rigid grid-band transforms preserve every entity.

use indexmap::IndexSet;

use super::*;

impl Axis {
    fn get(self, p: Point) -> i64 {
        match self {
            Self::X => p.x,
            Self::Y => p.y,
        }
    }
    fn set(self, p: &mut Point, value: i64) {
        match self {
            Self::X => p.x = value,
            Self::Y => p.y = value,
        }
    }
}

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
        let bp = self.routing_blueprint(state)?;
        for edge in &bp.edges {
            if let Some(cells) = &state.geometry.paths[edge.id] {
                geometry::check_path(&bp, &state.geometry, &self.catalogs.data, edge, cells)?;
            }
        }
        Ok(())
    }

    pub(super) fn detach(&self, state: &mut State) -> Result<()> {
        let bp = self.routing_blueprint(state)?;
        let detached: Vec<_> = bp
            .edges
            .iter()
            .filter(|e| {
                state.geometry.paths[e.id].as_ref().is_some_and(|cells| {
                    geometry::check_path(&bp, &state.geometry, &self.catalogs.data, e, cells)
                        .is_err()
                })
            })
            .map(|e| e.id)
            .collect();
        for id in detached {
            state.geometry.paths[id] = None;
        }
        Ok(())
    }

    pub(super) fn edit(&self, state: &mut State, action: Edit) -> Result<()> {
        match action {
            Edit::Place {
                instance,
                x,
                y,
                rotation,
                ports,
            } => {
                let ids = self.facility_nodes(&instance);
                if ids.iter().copied().collect::<IndexSet<_>>()
                    != ports.iter().map(|p| p.node).collect::<IndexSet<_>>()
                    || ports.len() != ids.len()
                {
                    return Err(error(
                        "port-assignment",
                        "Place requires exactly this facility's complete original terminal-to-port mapping",
                        instance,
                    ));
                }
                let p = state
                    .geometry
                    .placements
                    .iter_mut()
                    .find(|p| p.instance == instance)
                    .ok_or_else(|| {
                        error("facility", "Unknown original facility ID", instance.clone())
                    })?;
                let definition = self.catalogs.data.facilities.facility(&p.facility).unwrap();
                // Reject before rotation arithmetic; arbitrary integers are not catalog data.
                if !definition.allowed_rotations.contains(&rotation) {
                    return Err(error(
                        "pose",
                        "Position or rotation exceeds the catalog/coordinate domain",
                        instance,
                    ));
                }
                let (w, h, _) = catalog::rotate(definition, rotation, Point::default());
                p.x = x;
                p.y = y;
                p.width = w;
                p.height = h;
                p.rotation = rotation;
                for choice in ports {
                    state.ports[choice.node] = Some(choice.port);
                }
            }
            Edit::InsertBand { axis, at, count } => self.band(state, axis, at, count, false)?,
            Edit::DeleteBand { axis, at, count } => self.band(state, axis, at, count, true)?,
        }
        frame::canonicalize(&self.blueprint, state, &self.catalogs.data)?;
        self.check_entities(state)
    }

    fn band(&self, state: &mut State, axis: Axis, at: i64, count: i64, delete: bool) -> Result<()> {
        if !(1..=GUARD).contains(&count) || at.checked_add(count).is_none() {
            return Err(error(
                "band",
                "Band coordinate/count exceeds the coordinate work guard",
                json!({"at":at,"count":count}),
            ));
        }
        let cuts = |origin: i64, size: i64| {
            if delete {
                origin < at + count && origin + size > at
            } else {
                origin < at && origin + size > at
            }
        };
        for p in &state.geometry.placements {
            let (origin, size) = match axis {
                Axis::X => (p.x, p.width),
                Axis::Y => (p.y, p.height),
            };
            if cuts(origin, size) {
                return Err(error(
                    "band-cuts-body",
                    "Band would remove or split a rigid facility body",
                    p.instance.clone(),
                ));
            }
        }
        for (c, position) in self.blueprint.components.iter().zip(&state.components) {
            let definition = self
                .catalogs
                .data
                .components
                .component(&c.component)
                .unwrap();
            let (w, h) = if c.rotation % 180 == 0 {
                (definition.footprint.width, definition.footprint.height)
            } else {
                (definition.footprint.height, definition.footprint.width)
            };
            let size = match axis {
                Axis::X => w,
                Axis::Y => h,
            };
            if cuts(axis.get(Point::from(position)), size) {
                return Err(error(
                    "band-cuts-component",
                    "Band would remove or split an original logistics component",
                    c.id.clone(),
                ));
            }
        }
        if delete {
            for node in &self.blueprint.nodes {
                if !matches!(
                    node.terminal.as_ref().map(|t| &t.endpoint),
                    Some(TransportNetworkEndpoint::Facility { .. })
                ) && (at..at + count).contains(&axis.get(state.anchors[node.id]))
                {
                    return Err(error(
                        "band-removes-anchor",
                        "Band would remove an original external or component anchor",
                        node.id,
                    ));
                }
            }
        }
        let transform = |mut p: Point| -> Result<Option<Point>> {
            let v = axis.get(p);
            if delete && (at..at + count).contains(&v) {
                return Ok(None);
            }
            if v >= at {
                let value = v
                    .checked_add(if delete { -count } else { count })
                    .ok_or_else(|| {
                        error(
                            "coordinate-overflow",
                            "Band translation exceeds integer representation",
                            "band",
                        )
                    })?;
                axis.set(&mut p, value);
            }
            Ok(Some(p))
        };
        for p in &mut state.geometry.placements {
            let point = transform(Point { x: p.x, y: p.y })?.unwrap();
            p.x = point.x;
            p.y = point.y;
        }
        for point in &mut state.anchors {
            // Facility reference anchors are not physical obligations; their current
            // connection positions are derived from the selected catalog port.
            *point = transform(*point)?.unwrap_or_else(|| {
                let mut p = *point;
                axis.set(&mut p, at);
                p
            });
        }
        for p in &mut state.components {
            *p = transform(Point::from(&*p))?.unwrap().into();
        }
        for path in &mut state.geometry.paths {
            if let Some(cells) = path {
                let mut write = 0;
                for read in 0..cells.len() {
                    if let Some(point) = transform(cells[read])? {
                        cells[write] = point;
                        write += 1;
                    }
                }
                cells.truncate(write);
                if cells.is_empty() {
                    *path = None;
                }
            }
        }
        Ok(())
    }
}
