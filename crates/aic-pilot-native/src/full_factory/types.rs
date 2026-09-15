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

pub fn extent(w: &Witness) -> Result<Bounds> {
    let points = w
        .placements
        .iter()
        .flat_map(|p| {
            [
                Point { x: p.x, y: p.y },
                Point {
                    x: p.x + p.width - 1,
                    y: p.y + p.height - 1,
                },
            ]
        })
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
    bounds.ok_or_else(|| {
        Diagnostic::new(
            "factory-empty-geometry",
            "Cannot measure an empty witness",
            Value::Null,
        )
    })
}
