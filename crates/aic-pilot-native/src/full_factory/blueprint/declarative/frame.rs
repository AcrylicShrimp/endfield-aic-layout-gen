//! Checked occupied extents; global origins are not part of the physical problem.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(in super::super) struct PhysicalFrame {
    pub origin: [i128; 2],
    pub width: i64,
    pub height: i64,
}

#[derive(Default)]
struct Extent(Option<([i128; 2], [i128; 2])>);
impl Extent {
    fn point(&mut self, x: i128, y: i128) {
        let (low, high) = self.0.get_or_insert(([x, y], [x, y]));
        for (axis, value) in [x, y].into_iter().enumerate() {
            low[axis] = low[axis].min(value);
            high[axis] = high[axis].max(value);
        }
    }
    fn body(&mut self, x: i64, y: i64, width: i64, height: i64) -> Result<()> {
        if width <= 0 || height <= 0 {
            return Err(error(
                "coordinate-guard",
                "Positive physical footprint required",
                "frame",
            ));
        }
        self.point(x.into(), y.into());
        self.point(
            i128::from(x) + i128::from(width) - 1,
            i128::from(y) + i128::from(height) - 1,
        );
        Ok(())
    }
    fn finish(self) -> Result<PhysicalFrame> {
        let (origin, high) = self
            .0
            .ok_or_else(|| error("coordinate-guard", "Empty physical frame", "frame"))?;
        let [width, height] = std::array::from_fn(|i| high[i] - origin[i] + 1);
        if width - 1 > i128::from(GUARD) || height - 1 > i128::from(GUARD) {
            return Err(error(
                "coordinate-guard",
                "Occupied span exceeds the coordinate work guard",
                "frame",
            ));
        }
        Ok(PhysicalFrame {
            origin,
            width: width as i64,
            height: height as i64,
        })
    }
}

impl PhysicalFrame {
    pub fn from_points(points: impl IntoIterator<Item = Point>) -> Result<Self> {
        let mut extent = Extent::default();
        for p in points {
            extent.point(p.x.into(), p.y.into());
        }
        extent.finish()
    }

    fn point(self, x: i64, y: i64) -> Result<Point> {
        let coordinate = |value: i64, origin| {
            i64::try_from(i128::from(value) - origin).map_err(|_| {
                error(
                    "coordinate-overflow",
                    "Translated coordinate exceeds integer representation",
                    "frame",
                )
            })
        };
        Ok(Point {
            x: coordinate(x, self.origin[0])?,
            y: coordinate(y, self.origin[1])?,
        })
    }

    pub fn scale(self) -> f64 {
        self.width.max(self.height).max(1) as f64
    }
}

/// Admission uses i128 before chain extraction or world-space port arithmetic.
pub(in crate::full_factory) fn working_witness(
    original: &Witness,
    catalogs: &aic_data::layouts::FactoryEditCatalogs,
) -> Result<Witness> {
    let mut extent = Extent::default();
    for p in &original.placements {
        extent.body(p.x, p.y, p.width, p.height)?;
        let names = original
            .transport_networks
            .iter()
            .flat_map(|n| &n.terminals)
            .filter_map(|t| match &t.endpoint {
                TransportNetworkEndpoint::Facility { instance, port }
                    if instance == &p.instance =>
                {
                    Some(Ok(port.as_str()))
                }
                _ => None,
            });
        include_ports(&mut extent, p, names, catalogs)?;
    }
    for n in &original.transport_networks {
        for p in n
            .cells
            .iter()
            .chain(n.segments.iter().flat_map(|s| [&s.from, &s.to]))
            .chain(n.terminals.iter().map(|t| &t.position))
        {
            extent.point(p.x.into(), p.y.into());
        }
    }
    for c in &original.logistics_components {
        include_component(&mut extent, c, &c.position, catalogs)?;
    }
    let frame = extent.finish()?;
    let mut result = original.clone();
    for p in &mut result.placements {
        let q = frame.point(p.x, p.y)?;
        p.x = q.x;
        p.y = q.y;
    }
    for n in &mut result.transport_networks {
        for p in n
            .cells
            .iter_mut()
            .chain(n.segments.iter_mut().flat_map(|s| [&mut s.from, &mut s.to]))
            .chain(n.terminals.iter_mut().map(|t| &mut t.position))
        {
            *p = frame.point(p.x, p.y)?.into();
        }
    }
    for c in &mut result.logistics_components {
        c.position = frame.point(c.position.x, c.position.y)?.into();
    }
    Ok(result)
}

fn include_ports<'a>(
    extent: &mut Extent,
    p: &aic_data::layouts::FacilityPlacement,
    names: impl Iterator<Item = Result<&'a str>>,
    catalogs: &aic_data::layouts::FactoryEditCatalogs,
) -> Result<()> {
    let definition = catalogs
        .facilities
        .facility(&p.facility)
        .ok_or_else(|| error("facility", "Unknown facility", p.instance.clone()))?;
    if !definition.allowed_rotations.contains(&p.rotation) {
        return Err(error(
            "pose",
            "Rotation is outside the catalog domain",
            p.instance.clone(),
        ));
    }
    let (_, _, ports) = catalog::rotate(definition, p.rotation, Point::default());
    for name in names {
        let name = name?;
        let port = ports.iter().find(|port| port.id == name).ok_or_else(|| {
            error(
                "port-assignment",
                "Selected port is absent from its facility",
                name,
            )
        })?;
        for point in [port.position, port.connection] {
            extent.point(
                i128::from(p.x) + i128::from(point.x),
                i128::from(p.y) + i128::from(point.y),
            );
        }
    }
    Ok(())
}

fn include_component(
    extent: &mut Extent,
    component: &aic_data::layouts::PlacedLogisticsComponent,
    position: &WorldGridPosition,
    catalogs: &aic_data::layouts::FactoryEditCatalogs,
) -> Result<()> {
    let definition = catalogs
        .components
        .component(&component.component)
        .ok_or_else(|| error("component", "Unknown component", component.id.clone()))?;
    let (w, h) = (definition.footprint.width, definition.footprint.height);
    let (w, h) = if component.rotation % 180 == 0 {
        (w, h)
    } else {
        (h, w)
    };
    extent.body(position.x, position.y, w, h)
}

pub(super) fn from_state(
    bp: &Blueprint,
    state: &State,
    catalogs: &aic_data::layouts::FactoryEditCatalogs,
) -> Result<PhysicalFrame> {
    let mut extent = Extent::default();
    for p in &state.geometry.placements {
        extent.body(p.x, p.y, p.width, p.height)?;
        let names = bp
            .nodes
            .iter()
            .filter(|node| {
                matches!(node.terminal.as_ref().map(|t| &t.endpoint),
            Some(TransportNetworkEndpoint::Facility { instance, .. }) if instance == &p.instance)
            })
            .map(|node| {
                state
                    .ports
                    .get(node.id)
                    .and_then(Option::as_deref)
                    .ok_or_else(|| error("port-assignment", "Missing selected port", node.id))
            });
        include_ports(&mut extent, p, names, catalogs)?;
    }
    for node in &bp.nodes {
        if !matches!(
            node.terminal.as_ref().map(|t| &t.endpoint),
            Some(TransportNetworkEndpoint::Facility { .. })
        ) {
            let p = state
                .anchors
                .get(node.id)
                .ok_or_else(|| error("state-shape", "Missing anchor", node.id))?;
            extent.point(p.x.into(), p.y.into());
        }
    }
    if bp.components.len() != state.components.len() {
        return Err(error("state-shape", "Missing component positions", "state"));
    }
    for (c, p) in bp.components.iter().zip(&state.components) {
        include_component(&mut extent, c, p, catalogs)?;
    }
    for p in state.geometry.paths.iter().flatten().flatten() {
        extent.point(p.x.into(), p.y.into());
    }
    extent.finish()
}

/// Rebase the already-owned edit draft, including non-physical reference anchors.
pub(super) fn canonicalize(
    bp: &Blueprint,
    state: &mut State,
    catalogs: &aic_data::layouts::FactoryEditCatalogs,
) -> Result<PhysicalFrame> {
    let frame = from_state(bp, state, catalogs)?;
    if frame.origin != [0, 0] {
        for p in &mut state.geometry.placements {
            let q = frame.point(p.x, p.y)?;
            p.x = q.x;
            p.y = q.y;
        }
        for p in &mut state.anchors {
            *p = frame.point(p.x, p.y)?;
        }
        for p in &mut state.components {
            *p = frame.point(p.x, p.y)?.into();
        }
        for p in state.geometry.paths.iter_mut().flatten().flatten() {
            *p = frame.point(p.x, p.y)?;
        }
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_rectangle_extents_are_checked_before_narrowing() {
        let mut extent = Extent::default();
        extent.body(i64::MAX - 3, i64::MIN + 2, 4, 7).unwrap();
        let frame = extent.finish().unwrap();
        assert_eq!((frame.width, frame.height), (4, 7));
        assert_eq!(
            frame.point(i64::MAX - 3, i64::MIN + 2).unwrap(),
            Point::default()
        );
        let mut too_wide = Extent::default();
        too_wide.point(i64::MIN.into(), 0);
        too_wide.point(i64::MAX.into(), 0);
        assert!(too_wide.finish().is_err());
    }
}
