//! Retained candidate generation over the authoritative physical graph.

use super::*;

impl Learning {
    pub(super) fn build(&mut self, session: &Session) -> Result<Arc<Input>> {
        if let Some((revision, view)) = &mut self.cached
            && *revision == session.revision
        {
            if view.attempts != session.attempts || view.route_work != session.work.route_expansions
            {
                refresh_globals(
                    Arc::make_mut(&mut view.input),
                    session,
                    view.frame,
                    view.conflict_area,
                );
                view.attempts = session.attempts;
                view.route_work = session.work.route_expansions;
            }
            return Ok(view.input.clone());
        }
        let bp = &session.blueprint;
        let placements = &session.state.geometry.placements;
        let positions: Vec<_> = bp
            .nodes
            .iter()
            .map(|node| self.position(session, node))
            .collect::<Result<_>>()?;
        let mut xs: Vec<_> = positions.iter().map(|p| p.x).collect();
        let mut ys: Vec<_> = positions.iter().map(|p| p.y).collect();
        for f in placements {
            xs.extend([f.x, f.x + f.width - 1]);
            ys.extend([f.y, f.y + f.height - 1]);
        }
        for path in session.state.geometry.paths.iter().flatten() {
            for point in path {
                xs.push(point.x);
                ys.push(point.y);
            }
        }
        for component in &session.state.components {
            xs.push(component.x);
            ys.push(component.y);
        }
        let frame =
            frame::PhysicalFrame::from_points(xs.iter().zip(&ys).map(|(&x, &y)| Point { x, y }))?;
        let scale = frame.scale();
        let local_x = |x: i64| (i128::from(x) - frame.origin[0]) as f64 / scale;
        let local_y = |y: i64| (i128::from(y) - frame.origin[1]) as f64 / scale;
        let mut collision_counts = vec![0.; placements.len()];
        let mut conflict_area = 0.;
        let mut conflicts = 0;
        for (i, a) in placements.iter().enumerate() {
            for (j, b) in placements.iter().enumerate().skip(i + 1) {
                let width = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
                let height = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
                if width > 0 && height > 0 {
                    conflicts += 1;
                    collision_counts[i] += 1.;
                    collision_counts[j] += 1.;
                    conflict_area += (width * height) as f64;
                }
            }
            for c in &session.state.components {
                if c.x >= a.x && c.x < a.x + a.width && c.y >= a.y && c.y < a.y + a.height {
                    conflicts += 1;
                    collision_counts[i] += 1.;
                    conflict_area += 1.;
                }
            }
        }
        let mut value = self.physical(session, &positions, frame, &collision_counts, conflicts)?;

        self.roots.clear();
        for (owner, f) in placements.iter().enumerate() {
            let mut destinations = BTreeSet::from([(f.x, f.y)]);
            for &step in &self.options.translation_steps {
                if let Some(x) = f.x.checked_add(step) {
                    destinations.insert((x, f.y));
                }
                if let Some(y) = f.y.checked_add(step) {
                    destinations.insert((f.x, y));
                }
            }
            destinations.extend(placements.iter().map(|other| (other.x, other.y)));
            let definition = session
                .catalogs
                .data
                .facilities
                .facility(&f.facility)
                .unwrap();
            for (x, y) in destinations {
                for &rotation in &self.facilities[owner].rotations {
                    let (mut w, mut h) = (definition.footprint.width, definition.footprint.height);
                    if rotation % 180 != 0 {
                        std::mem::swap(&mut w, &mut h)
                    }
                    let (sin, cos) = angle(rotation);
                    self.roots.push(Root {
                        kind: 0,
                        invalid: false,
                        owner,
                        x,
                        y,
                        rotation,
                        axis: Axis::X,
                        at: 0,
                        count: 0,
                    });
                    value.action_features.push(floats([
                        1.,
                        0.,
                        0.,
                        (x - f.x) as f64 / scale,
                        (y - f.y) as f64 / scale,
                        sin,
                        cos,
                        w as f64 / scale,
                        h as f64 / scale,
                        0.,
                        0.,
                        0.,
                    ]));
                    value
                        .action_references
                        .push([self.tables.facilities[owner] as i64; 2]);
                    value.root_meta.push([0, owner as i64, rotation]);
                    value.root_invalid.push(false);
                }
            }
        }
        let mut bodies: Vec<_> = placements
            .iter()
            .map(|f| [f.x, f.y, f.width, f.height])
            .collect();
        for (definition, position) in bp.components.iter().zip(&session.state.components) {
            let component = session
                .catalogs
                .data
                .components
                .component(&definition.component)
                .unwrap();
            let (mut w, mut h) = (component.footprint.width, component.footprint.height);
            if definition.rotation % 180 != 0 {
                std::mem::swap(&mut w, &mut h)
            }
            bodies.push([position.x, position.y, w, h]);
        }
        let anchors: Vec<_> = bp
            .nodes
            .iter()
            .filter(|n| {
                !matches!(
                    n.terminal.as_ref().map(|t| &t.endpoint),
                    Some(TransportNetworkEndpoint::Facility { .. })
                )
            })
            .map(|n| positions[n.id])
            .collect();
        for kind in 1..=2 {
            for (axis, points) in [(Axis::X, &xs), (Axis::Y, &ys)] {
                for at in *points.iter().min().unwrap()..=*points.iter().max().unwrap() {
                    for &count in &self.options.band_counts {
                        let coordinate = usize::from(matches!(axis, Axis::Y));
                        let invalid = self.options.masking.is_some()
                            && (bodies.iter().any(|body| {
                                let (origin, size) = (body[coordinate], body[coordinate + 2]);
                                if kind == 1 {
                                    origin < at && at < origin + size
                                } else {
                                    at < origin + size && at + count > origin
                                }
                            }) || kind == 2
                                && anchors.iter().any(|p| {
                                    let n = if coordinate == 0 { p.x } else { p.y };
                                    at <= n && n < at + count
                                }));
                        self.roots.push(Root {
                            kind,
                            invalid,
                            owner: 0,
                            x: 0,
                            y: 0,
                            rotation: 0,
                            axis,
                            at,
                            count,
                        });
                        value.action_features.push(floats([
                            0.,
                            f64::from(kind == 1),
                            f64::from(kind == 2),
                            0.,
                            0.,
                            0.,
                            1.,
                            0.,
                            0.,
                            coordinate as f64,
                            if coordinate == 0 {
                                local_x(at)
                            } else {
                                local_y(at)
                            },
                            count as f64 / scale,
                        ]));
                        value.action_references.push([-1; 2]);
                        value.root_meta.push([kind as i64, -1, 0]);
                        value.root_invalid.push(invalid);
                    }
                }
            }
        }
        for f in &self.facilities {
            for group in &f.groups {
                for &rotation in &f.rotations {
                    for name in &group.ports {
                        let port = f.ports[&rotation].iter().find(|p| &p.id == name).unwrap();
                        let original =
                            bp.nodes[group.node].terminal.as_ref().and_then(|t| {
                                match &t.endpoint {
                                    TransportNetworkEndpoint::Facility { port, .. } => Some(port),
                                    _ => None,
                                }
                            });
                        let direction = port.connection.shifted(-port.position.x, -port.position.y);
                        value.port_features.push(floats([
                            port.connection.x as f64 / scale,
                            port.connection.y as f64 / scale,
                            direction.x as f64,
                            direction.y as f64,
                            f64::from(port.direction == FacilityPortDirection::Input),
                            f64::from(port.direction == FacilityPortDirection::Output),
                            f64::from(view::transport(port.transport) == 0),
                            f64::from(view::transport(port.transport) == 1),
                            f64::from(session.state.ports[group.node].as_ref() == Some(name)),
                            f64::from(original == Some(name)),
                            self.tables.capacities[view::transport(port.transport)] as f64,
                        ]));
                    }
                }
            }
        }
        let mut counts = [0usize; 3];
        for root in &self.roots {
            counts[root.kind] += 1
        }
        let logs = counts.map(|count| (count as f64).ln() as f32);
        value.kind_log_counts = self.roots.iter().map(|root| logs[root.kind]).collect();
        refresh_globals(&mut value, session, frame, conflict_area);
        let input = Arc::new(value);
        self.cached = Some((
            session.revision,
            view::PhysicalView {
                input: input.clone(),
                frame,
                conflict_area,
                attempts: session.attempts,
                route_work: session.work.route_expansions,
            },
        ));
        Ok(input)
    }
}

fn refresh_globals(
    value: &mut Input,
    session: &Session,
    frame: frame::PhysicalFrame,
    conflict_area: f64,
) {
    value.global_features.clear();
    value.global_features.extend(
        session
            .best_metrics
            .score
            .iter()
            .zip(&session.initial_metrics.score)
            .map(|(&b, &a)| (b as f64 / a.max(1) as f64) as f32),
    );
    value.global_features.extend(
        session
            .initial_metrics
            .score
            .iter()
            .map(|&a| (a as f64).ln_1p() as f32),
    );
    let remaining = session
        .config
        .max_total_route_expansions
        .saturating_sub(session.work.route_expansions);
    value.global_features.extend([
        value.pending as f32 / session.blueprint.edges.len().max(1) as f32,
        (conflict_area / session.initial_metrics.score[0].max(1) as f64) as f32,
        session.config.max_actions.saturating_sub(session.attempts) as f32
            / session.config.max_actions as f32,
        remaining as f32 / session.config.max_total_route_expansions as f32,
        (remaining.min(session.config.route_expansions) as f64).ln_1p() as f32,
        (session.config.search_padding as f64 / frame.scale()) as f32,
        (frame.width as f64).ln_1p() as f32,
        (frame.height as f64).ln_1p() as f32,
        (1. / frame.scale()) as f32,
    ]);
    debug_assert_eq!(value.global_features.len(), GLOBAL_FEATURES);
}
