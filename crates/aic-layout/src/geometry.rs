//! Integer grid geometry shared by all layout engines.
use crate::logistics::TransportKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
impl From<&Point> for Point {
    fn from(p: &Point) -> Self {
        *p
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
