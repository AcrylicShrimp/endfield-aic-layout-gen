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

        let port_groups = self
            .facilities
            .iter()
            .map(|facility| {
                facility
                    .groups
                    .iter()
                    .map(|group| crate::physical_editor::PortDomain {
                        node: group.node,
                        ports: group.ports.clone(),
                    })
                    .collect()
            })
            .collect();
        self.handle = self
            .candidates
            .enumerate_with_port_groups(
                &session.common_problem,
                &session.common_state,
                &crate::physical_editor::PhysicalCandidateOptions {
                    translation_steps: self.options.translation_steps.clone(),
                    band_counts: self.options.band_counts.clone(),
                    structural_band_mask: self.options.masking.is_some(),
                },
                session.revision,
                session.attempts,
                port_groups,
            )
            .map_err(|failure| error("learning-domain", failure.message, failure.entity))?;
        let common_roots = self.candidates.roots().to_vec();
        self.roots.clear();
        let mut counts = [0usize; 3];
        for root in common_roots {
            match root.action {
                crate::physical_editor::PhysicalRootAction::Place {
                    block: owner,
                    position: Point { x, y },
                    rotation,
                    ..
                } => {
                    let f = &placements[owner];
                    let definition = session.common_facility(&f.facility);
                    let (mut w, mut h) = (definition.footprint.width, definition.footprint.height);
                    if rotation % 180 != 0 {
                        std::mem::swap(&mut w, &mut h)
                    }
                    let (sin, cos) = angle(rotation);
                    counts[0] += usize::from(!root.invalid);
                    self.roots.push(Root {
                        kind: 0,
                        invalid: root.invalid,
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
                    value.root_invalid.push(root.invalid);
                }
                action => {
                    let (kind, axis, at, count) = match action {
                        crate::physical_editor::PhysicalRootAction::InsertBand {
                            axis,
                            at,
                            count,
                        } => (1, axis, at, count),
                        crate::physical_editor::PhysicalRootAction::DeleteBand {
                            axis,
                            at,
                            count,
                        } => (2, axis, at, count),
                        crate::physical_editor::PhysicalRootAction::Place { .. } => unreachable!(),
                    };
                    let axis = match axis {
                        crate::physical_editor::Axis::X => Axis::X,
                        crate::physical_editor::Axis::Y => Axis::Y,
                    };
                    let coordinate = usize::from(matches!(axis, Axis::Y));
                    counts[kind] += usize::from(!root.invalid);
                    self.roots.push(Root {
                        kind,
                        invalid: root.invalid,
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
                    value.root_invalid.push(root.invalid);
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
        if counts.iter().all(|&n| n == 0) {
            return Err(error(
                "learning-empty-actions",
                "No unmasked root action is available",
                session.blueprint_sha256.clone(),
            ));
        }
        let logs = counts.map(|count| (count.max(1) as f64).ln() as f32);
        value.kind_log_counts = self
            .roots
            .iter()
            .map(|root| if root.invalid { 0. } else { logs[root.kind] })
            .collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn factory_and_procedural_paths_share_candidate_roots_and_band_edits() {
        let session = super::super::super::tests::session();
        let options = Options {
            translation_steps: vec![-1, 1],
            band_counts: vec![1],
            masking: Some("structural-bands-v1".into()),
        };
        let mut factory = Learning::new(&session, options).unwrap();
        factory.observe(&session).unwrap();

        let procedural_options = crate::physical_editor::PhysicalCandidateOptions {
            translation_steps: vec![-1, 1],
            band_counts: vec![1],
            structural_band_mask: true,
        };
        let mut procedural = crate::physical_editor::PhysicalCandidateAuthority::default();
        let domain = procedural
            .enumerate(
                &session.common_problem,
                &session.common_state,
                &procedural_options,
                session.revision,
                session.attempts,
            )
            .unwrap();
        assert_eq!(factory.candidates.roots(), domain.roots);
        assert_eq!(factory.candidates.port_groups(), domain.port_groups);

        let place_root = domain
            .roots
            .iter()
            .position(|root| {
                matches!(
                    root.action,
                    crate::physical_editor::PhysicalRootAction::Place { block: 0, .. }
                )
            })
            .unwrap();
        let selected_ports = domain.port_groups[0]
            .iter()
            .map(|port_domain| {
                let selected = session.common_state.selected_ports[port_domain.node]
                    .as_ref()
                    .unwrap();
                port_domain
                    .ports
                    .iter()
                    .position(|port| port == selected)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let factory_place = factory
            .candidates
            .select(
                factory.handle,
                session.revision,
                session.attempts,
                place_root,
                &selected_ports,
            )
            .unwrap();
        let procedural_place = procedural
            .select(
                domain.handle,
                session.revision,
                session.attempts,
                place_root,
                &selected_ports,
            )
            .unwrap();
        assert_eq!(factory_place, procedural_place);

        let root = domain
            .roots
            .iter()
            .position(|root| {
                !root.invalid
                    && matches!(
                        root.action,
                        crate::physical_editor::PhysicalRootAction::InsertBand { .. }
                            | crate::physical_editor::PhysicalRootAction::DeleteBand { .. }
                    )
            })
            .unwrap();
        let factory_edit = factory
            .candidates
            .select(
                factory.handle,
                session.revision,
                session.attempts,
                root,
                &[],
            )
            .unwrap();
        let procedural_edit = procedural
            .select(domain.handle, session.revision, session.attempts, root, &[])
            .unwrap();
        assert_eq!(factory_edit, procedural_edit);
        let mut factory_draft = session.common_state.clone();
        let mut procedural_draft = session.common_state.clone();
        crate::physical_editor::apply_edit(
            &session.common_problem,
            &mut factory_draft,
            factory_edit,
        )
        .unwrap();
        crate::physical_editor::apply_edit(
            &session.common_problem,
            &mut procedural_draft,
            procedural_edit,
        )
        .unwrap();
        let limits = crate::physical_editor::PhysicalEditLimits {
            route_expansions: 100_000,
            search_padding: 5,
        };
        let factory_work = crate::physical_editor::repair_pending(
            &session.common_problem,
            &mut factory_draft,
            limits,
        )
        .unwrap();
        let procedural_work = crate::physical_editor::repair_pending(
            &session.common_problem,
            &mut procedural_draft,
            limits,
        )
        .unwrap();
        assert_eq!(factory_work, procedural_work);
        assert_eq!(factory_draft, procedural_draft);
    }

    #[test]
    fn available_family_counts_preserve_indices_and_neutralize_masked_roots() {
        let session = super::super::super::tests::session();
        let options = Options {
            translation_steps: vec![-1, 1],
            band_counts: vec![1],
            masking: None,
        };
        let mut unmasked = Learning::new(&session, options.clone()).unwrap();
        let all = unmasked.observe(&session).unwrap();
        let mut masked = Learning::new(
            &session,
            Options {
                masking: Some("structural-bands-v1".into()),
                ..options.clone()
            },
        )
        .unwrap();
        let input = masked.observe(&session).unwrap();
        assert_eq!(all.root_meta, input.root_meta);
        assert_eq!(all.action_features, input.action_features);
        assert_eq!(all.action_references, input.action_references);
        assert_eq!(all.port_features, input.port_features);
        let mut counts = [0usize; 3];
        for (meta, &invalid) in input.root_meta.iter().zip(&input.root_invalid) {
            counts[meta[0] as usize] += usize::from(!invalid);
        }
        assert!(counts.iter().all(|&n| n > 0));
        assert!(input.root_invalid.iter().any(|&v| v));
        for ((meta, &invalid), &correction) in input
            .root_meta
            .iter()
            .zip(&input.root_invalid)
            .zip(&input.kind_log_counts)
        {
            assert_eq!(
                correction,
                if invalid {
                    0.
                } else {
                    (counts[meta[0] as usize] as f64).ln() as f32
                }
            );
            assert!(correction.is_finite());
        }
        let mut only_place = Learning::new(
            &session,
            Options {
                band_counts: vec![],
                ..options
            },
        )
        .unwrap();
        let input = only_place.observe(&session).unwrap();
        assert!(input.root_meta.iter().all(|m| m[0] == 0));
        assert!(
            input
                .kind_log_counts
                .iter()
                .all(|&v| v == (input.root_meta.len() as f64).ln() as f32)
        );
    }
}
