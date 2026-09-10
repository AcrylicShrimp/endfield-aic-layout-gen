use aic_data::layouts::{FacilityPlacement, WorldGridPosition};
use aic_data::logistics::TransportKind;
use aic_data::recipes::Rate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Witness = aic_data::layouts::WitnessProbeInput;
pub type LayerPoint = (usize, Point);
pub type Result<T> = std::result::Result<T, Diagnostic>;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

impl Point {
    pub fn shifted(self, dx: i64, dy: i64) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }
    pub fn offset(self, direction: usize) -> Self {
        let (dx, dy) = DIRECTIONS[direction];
        self.shifted(dx, dy)
    }
    pub fn distance(self, other: Self) -> i64 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }
}
impl From<&WorldGridPosition> for Point {
    fn from(p: &WorldGridPosition) -> Self {
        Self { x: p.x, y: p.y }
    }
}
impl From<Point> for WorldGridPosition {
    fn from(p: Point) -> Self {
        Self { x: p.x, y: p.y }
    }
}

pub const DIRECTIONS: [(i64, i64); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
pub fn direction(a: Point, b: Point) -> Option<usize> {
    DIRECTIONS
        .iter()
        .position(|&(x, y)| b.x - a.x == x && b.y - a.y == y)
}
pub fn layer(transport: TransportKind) -> usize {
    match transport {
        TransportKind::Belt => 0,
        TransportKind::Pipe => 1,
    }
}
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub min_x: i64,
    pub min_y: i64,
    pub max_x: i64,
    pub max_y: i64,
}
impl Bounds {
    pub fn width(self) -> i64 {
        self.max_x - self.min_x + 1
    }
    pub fn height(self) -> i64 {
        self.max_y - self.min_y + 1
    }
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.min_x && p.x <= self.max_x && p.y >= self.min_y && p.y <= self.max_y
    }
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

#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: &'static str,
    pub stage: &'static str,
    pub message: String,
    pub entity: Value,
}
impl Diagnostic {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        entity: impl Into<Value>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: "error",
            stage: "local-reembedding",
            message: message.into(),
            entity: entity.into(),
        }
    }
}
