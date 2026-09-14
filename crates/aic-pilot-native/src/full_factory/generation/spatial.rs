use indexmap::IndexSet;
use serde::Serialize;

use super::types::*;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Rectangle {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}
pub struct Prefix {
    bounds: Bounds,
    stride: usize,
    data: Vec<i32>,
}
impl Prefix {
    pub fn new(bounds: Bounds, points: impl IntoIterator<Item = Point>) -> Self {
        let width = bounds.width() as usize;
        let height = bounds.height() as usize;
        let stride = width + 1;
        let mut data = vec![0; stride * (height + 1)];
        for p in points {
            if bounds.contains(p) {
                data[(p.y - bounds.min_y + 1) as usize * stride
                    + (p.x - bounds.min_x + 1) as usize] = 1;
            }
        }
        for y in 1..=height {
            for x in 1..=width {
                data[y * stride + x] += data[y * stride + x - 1] + data[(y - 1) * stride + x]
                    - data[(y - 1) * stride + x - 1];
            }
        }
        Self {
            bounds,
            stride,
            data,
        }
    }
    pub fn occupied(&self, x: i64, y: i64, width: i64, height: i64) -> bool {
        let x = x - self.bounds.min_x;
        let y = y - self.bounds.min_y;
        if x < 0 || y < 0 || x + width > self.bounds.width() || y + height > self.bounds.height() {
            return true;
        }
        let x = x as usize;
        let y = y as usize;
        let width = width as usize;
        let height = height as usize;
        let s = self.stride;
        self.data[(y + height) * s + x + width]
            - self.data[y * s + x + width]
            - self.data[(y + height) * s + x]
            + self.data[y * s + x]
            != 0
    }
}
pub fn envelope(points: impl IntoIterator<Item = Rectangle>) -> Rectangle {
    let mut x = i64::MAX;
    let mut y = i64::MAX;
    let mut right = i64::MIN;
    let mut bottom = i64::MIN;
    for p in points {
        x = x.min(p.x);
        y = y.min(p.y);
        right = right.max(p.x + p.width);
        bottom = bottom.max(p.y + p.height);
    }
    Rectangle {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}
pub fn empty_rectangles(w: &Witness, bounds: Bounds) -> Vec<Rectangle> {
    let occupied: IndexSet<_> = w
        .placements
        .iter()
        .flat_map(body_cells)
        .chain(
            w.transport_networks
                .iter()
                .flat_map(|n| n.cells.iter().map(Point::from)),
        )
        .chain(
            w.logistics_components
                .iter()
                .map(|c| Point::from(&c.position)),
        )
        .collect();
    let width = bounds.width() as usize;
    let mut heights = vec![0_i64; width];
    let mut rectangles = Vec::new();
    for y in bounds.min_y..=bounds.max_y {
        for (x, h) in heights.iter_mut().enumerate() {
            *h = if occupied.contains(&Point {
                x: x as i64 + bounds.min_x,
                y,
            }) {
                0
            } else {
                *h + 1
            };
        }
        let mut stack = Vec::<(usize, i64)>::new();
        for x in 0..=width {
            let h = if x == width { 0 } else { heights[x] };
            let mut left = x;
            while stack.last().is_some_and(|&(_, sh)| sh > h) {
                let (sl, sh) = stack.pop().unwrap();
                left = sl;
                rectangles.push(Rectangle {
                    x: bounds.min_x + sl as i64,
                    y: y - sh + 1,
                    width: (x - sl) as i64,
                    height: sh,
                });
            }
            if h > 0 && stack.last().is_none_or(|&(_, sh)| sh < h) {
                stack.push((left, h));
            }
        }
    }
    rectangles.sort_by_key(|r| (-(r.width * r.height), r.y, r.x));
    let mut result: Vec<Rectangle> = Vec::new();
    for r in rectangles {
        if result.iter().any(|p| {
            r.x >= p.x
                && r.y >= p.y
                && r.x + r.width <= p.x + p.width
                && r.y + r.height <= p.y + p.height
        }) {
            continue;
        }
        result.push(r);
        if result.len() == 8 {
            break;
        }
    }
    result
}
pub fn stride(total: i64) -> i64 {
    fn gcd(a: i64, b: i64) -> i64 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let mut stride = ((total as f64 * 0.61803398875).floor() as i64).max(1);
    while gcd(stride, total) != 1 {
        stride += 1;
    }
    stride
}
