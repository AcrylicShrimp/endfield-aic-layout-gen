//! Retained candidate generation over the authoritative physical graph.

use super::*;

impl Learning {
    pub(super) fn build(
        &mut self,
        draft: &PhysicalDraftV1,
        revision: u64,
        context: Context<'_>,
        options: &PhysicalCandidateOptions,
    ) -> Result<Arc<Input>> {
        let edge_count = self.problem.edges.len();
        if let Some((cached_revision, cached_options, view)) = &mut self.cached
            && *cached_revision == revision
            && cached_options == options
        {
            if view.attempts != context.attempts || view.route_work != context.route_work {
                refresh_globals(
                    Arc::make_mut(&mut view.input),
                    context,
                    view.frame,
                    view.conflict_area,
                    edge_count,
                );
                view.attempts = context.attempts;
                view.route_work = context.route_work;
            }
            return Ok(view.input.clone());
        }
        let problem = self.problem.clone();
        let placements = &draft.poses;
        let positions: Vec<_> = problem
            .nodes
            .iter()
            .map(|node| crate::physical_editor::node_position(&problem, draft, node))
            .collect::<Result<_>>()?;
        let mut xs: Vec<_> = positions.iter().map(|p| p.x).collect();
        let mut ys: Vec<_> = positions.iter().map(|p| p.y).collect();
        for (owner, pose) in placements.iter().enumerate() {
            let definition = facility(&problem, &problem.blocks[owner].facility);
            let (width, height) = if pose.rotation % 180 == 0 {
                (definition.footprint.width, definition.footprint.height)
            } else {
                (definition.footprint.height, definition.footprint.width)
            };
            xs.extend([pose.position.x, pose.position.x + width - 1]);
            ys.extend([pose.position.y, pose.position.y + height - 1]);
        }
        for path in draft.paths.iter().flatten() {
            for point in path {
                xs.push(point.x);
                ys.push(point.y);
            }
        }
        for component in &draft.component_positions {
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
        let rectangles: Vec<_> = placements
            .iter()
            .enumerate()
            .map(|(owner, pose)| {
                let definition = facility(&problem, &problem.blocks[owner].facility);
                let (width, height) = if pose.rotation % 180 == 0 {
                    (definition.footprint.width, definition.footprint.height)
                } else {
                    (definition.footprint.height, definition.footprint.width)
                };
                (pose.position.x, pose.position.y, width, height)
            })
            .collect();
        for (i, &(ax, ay, aw, ah)) in rectangles.iter().enumerate() {
            for (j, &(bx, by, bw, bh)) in rectangles.iter().enumerate().skip(i + 1) {
                let width = (ax + aw).min(bx + bw) - ax.max(bx);
                let height = (ay + ah).min(by + bh) - ay.max(by);
                if width > 0 && height > 0 {
                    conflicts += 1;
                    collision_counts[i] += 1.;
                    collision_counts[j] += 1.;
                    conflict_area += (width * height) as f64;
                }
            }
            for c in &draft.component_positions {
                if c.x >= ax && c.x < ax + aw && c.y >= ay && c.y < ay + ah {
                    conflicts += 1;
                    collision_counts[i] += 1.;
                    conflict_area += 1.;
                }
            }
        }
        let mut value = self.physical(
            &problem,
            draft,
            &positions,
            frame,
            &collision_counts,
            conflicts,
        )?;

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
                    let f = placements[owner];
                    let definition = facility(&problem, &problem.blocks[owner].facility);
                    let (mut w, mut h) = (definition.footprint.width, definition.footprint.height);
                    if rotation % 180 != 0 {
                        std::mem::swap(&mut w, &mut h)
                    }
                    let (sin, cos) = angle(rotation);
                    counts[0] += usize::from(!root.invalid);
                    self.roots.push(Root {
                        kind: 0,
                        invalid: root.invalid,
                    });
                    value.action_features.push(floats([
                        1.,
                        0.,
                        0.,
                        (x - f.position.x) as f64 / scale,
                        (y - f.position.y) as f64 / scale,
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
                    let coordinate = usize::from(matches!(axis, crate::physical_editor::Axis::Y));
                    counts[kind] += usize::from(!root.invalid);
                    self.roots.push(Root {
                        kind,
                        invalid: root.invalid,
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
                            problem.nodes[group.node]
                                .terminal
                                .as_ref()
                                .and_then(|terminal| match terminal {
                                    CompiledTerminal::Facility { original_port, .. } => {
                                        Some(original_port)
                                    }
                                    CompiledTerminal::External { .. } => None,
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
                            f64::from(draft.selected_ports[group.node].as_ref() == Some(name)),
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
                context.source_sha256.to_owned(),
                "No unmasked root action is available",
            ));
        }
        let logs = counts.map(|count| (count.max(1) as f64).ln() as f32);
        value.kind_log_counts = self
            .roots
            .iter()
            .map(|root| if root.invalid { 0. } else { logs[root.kind] })
            .collect();
        refresh_globals(&mut value, context, frame, conflict_area, edge_count);
        let input = Arc::new(value);
        self.cached = Some((
            revision,
            options.clone(),
            view::PhysicalView {
                input: input.clone(),
                frame,
                conflict_area,
                attempts: context.attempts,
                route_work: context.route_work,
            },
        ));
        Ok(input)
    }
}

fn refresh_globals(
    value: &mut Input,
    context: Context<'_>,
    frame: frame::PhysicalFrame,
    conflict_area: f64,
    edge_count: usize,
) {
    value.global_features.clear();
    value.global_features.extend(
        context
            .best_score
            .iter()
            .zip(&context.initial_score)
            .map(|(&b, &a)| (b as f64 / a.max(1) as f64) as f32),
    );
    value.global_features.extend(
        context
            .initial_score
            .iter()
            .map(|&a| (a as f64).ln_1p() as f32),
    );
    let remaining = context
        .max_total_route_work
        .saturating_sub(context.route_work);
    value.global_features.extend([
        value.pending as f32 / edge_count.max(1) as f32,
        (conflict_area / context.initial_score[0].max(1) as f64) as f32,
        context.max_actions.saturating_sub(context.attempts) as f32 / context.max_actions as f32,
        remaining as f32 / context.max_total_route_work as f32,
        (remaining.min(context.per_action_route_work) as f64).ln_1p() as f32,
        (context.search_padding as f64 / frame.scale()) as f32,
        (frame.width as f64).ln_1p() as f32,
        (frame.height as f64).ln_1p() as f32,
        (1. / frame.scale()) as f32,
    ]);
    debug_assert_eq!(value.global_features.len(), GLOBAL_FEATURES);
}
