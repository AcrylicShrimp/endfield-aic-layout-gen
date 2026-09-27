use aic_data::layouts::FacilityPlacement;
use aic_data::recipes::Rate;
pub use aic_layout::geometry::{Bounds, Diagnostic, Point, direction, layer};
use serde_json::Value;

pub type Witness = aic_data::layouts::WitnessProbeInput;
pub type LayerPoint = (usize, Point);
pub type Result<T> = std::result::Result<T, Diagnostic>;

pub fn equal_rate(a: Rate, b: Rate) -> bool {
    i128::from(a.numerator) * i128::from(b.denominator)
        == i128::from(b.numerator) * i128::from(a.denominator)
}
pub fn positive_rate(r: Rate) -> bool {
    r.numerator > 0 && r.denominator > 0
}
pub fn body_cells(p: &FacilityPlacement) -> impl Iterator<Item = Point> + '_ {
    (p.y..p.y + p.height).flat_map(move |y| (p.x..p.x + p.width).map(move |x| Point { x, y }))
}
fn extent_resource(message: &str) -> Diagnostic {
    let mut e = Diagnostic::new(
        "factory.vocabulary-resource-incomplete",
        message,
        Value::Null,
    );
    e.stage = "factory-geometry";
    e
}

pub fn extent(w: &Witness) -> Result<Bounds> {
    let mut facility_corners = Vec::new();
    let corners = w
        .placements
        .len()
        .checked_mul(2)
        .ok_or_else(|| extent_resource("Facility corner count cannot be represented"))?;
    facility_corners
        .try_reserve_exact(corners)
        .map_err(|_| extent_resource("Facility corner allocation refused"))?;
    for p in &w.placements {
        let right = p.x.checked_add(p.width).and_then(|v| v.checked_sub(1));
        let bottom = p.y.checked_add(p.height).and_then(|v| v.checked_sub(1));
        let (Some(right), Some(bottom)) = (right, bottom) else {
            return Err(extent_resource("Facility extent cannot be represented"));
        };
        facility_corners.extend([
            Point { x: p.x, y: p.y },
            Point {
                x: right,
                y: bottom,
            },
        ]);
    }
    let points = facility_corners
        .into_iter()
        .chain(
            w.transport_networks
                .iter()
                .flat_map(|n| n.cells.iter().map(Point::from)),
        )
        .chain(
            w.logistics_components
                .iter()
                .map(|c| Point::from(&c.position)),
        );
    let mut bounds = None::<Bounds>;
    for p in points {
        if let Some(b) = &mut bounds {
            b.min_x = b.min_x.min(p.x);
            b.max_x = b.max_x.max(p.x);
            b.min_y = b.min_y.min(p.y);
            b.max_y = b.max_y.max(p.y);
        } else {
            bounds = Some(Bounds {
                min_x: p.x,
                max_x: p.x,
                min_y: p.y,
                max_y: p.y,
            });
        }
    }
    let bounds = bounds.ok_or_else(|| {
        Diagnostic::new(
            "factory-empty-geometry",
            "Cannot measure an empty witness",
            Value::Null,
        )
    })?;
    if i64::try_from(i128::from(bounds.max_x) - i128::from(bounds.min_x) + 1).is_err()
        || i64::try_from(i128::from(bounds.max_y) - i128::from(bounds.min_y) + 1).is_err()
    {
        return Err(extent_resource("Canvas extent cannot be represented"));
    }
    Ok(bounds)
}
