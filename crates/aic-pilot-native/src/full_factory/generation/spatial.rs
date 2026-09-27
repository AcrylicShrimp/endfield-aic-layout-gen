use aic_data::layouts::FacilityPlacement;
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
    storage: PrefixStorage,
}
enum PrefixStorage {
    Dense(Vec<i32>),
    Sparse(IndexSet<Point>),
}
impl Prefix {
    pub fn try_new(bounds: Bounds, points: impl IntoIterator<Item = Point>) -> Result<Self> {
        let width = usize::try_from(i128::from(bounds.max_x) - i128::from(bounds.min_x) + 1)
            .map_err(|_| resource("Spatial width cannot be represented"))?;
        let height = usize::try_from(i128::from(bounds.max_y) - i128::from(bounds.min_y) + 1)
            .map_err(|_| resource("Spatial height cannot be represented"))?;
        let stride = width
            .checked_add(1)
            .ok_or_else(|| resource("Spatial stride overflow"))?;
        let len = stride
            .checked_mul(
                height
                    .checked_add(1)
                    .ok_or_else(|| resource("Spatial height overflow"))?,
            )
            .ok_or_else(|| resource("Spatial buffer size overflow"))?;
        let mut collected = Vec::new();
        for p in points.into_iter().filter(|p| bounds.contains(*p)) {
            if collected.len() == collected.capacity() {
                collected
                    .try_reserve(1)
                    .map_err(|_| resource("Spatial point allocation refused"))?;
            }
            collected.push(p);
        }
        // A sparse representation keeps a very large, mostly empty canvas from
        // reserving memory proportional to its bounding box.
        if len > 1_000_000 && len > collected.len().saturating_mul(32) {
            let mut sparse = IndexSet::new();
            for p in collected {
                if sparse.len() == sparse.capacity() && !sparse.contains(&p) {
                    sparse
                        .try_reserve(1)
                        .map_err(|_| resource("Sparse spatial index allocation refused"))?;
                }
                sparse.insert(p);
            }
            return Ok(Self {
                bounds,
                stride,
                storage: PrefixStorage::Sparse(sparse),
            });
        }
        let mut data = Vec::new();
        data.try_reserve_exact(len)
            .map_err(|_| resource("Spatial buffer allocation refused"))?;
        data.resize(len, 0_i32);
        for p in collected {
            data[(p.y - bounds.min_y + 1) as usize * stride + (p.x - bounds.min_x + 1) as usize] =
                1;
        }
        for y in 1..=height {
            for x in 1..=width {
                data[y * stride + x] += data[y * stride + x - 1] + data[(y - 1) * stride + x]
                    - data[(y - 1) * stride + x - 1];
            }
        }
        Ok(Self {
            bounds,
            stride,
            storage: PrefixStorage::Dense(data),
        })
    }
    pub fn count(&self, x: i64, y: i64, width: i64, height: i64) -> Option<i64> {
        let relative_x = i128::from(x) - i128::from(self.bounds.min_x);
        let relative_y = i128::from(y) - i128::from(self.bounds.min_y);
        let x = i64::try_from(relative_x).ok()?;
        let y = i64::try_from(relative_y).ok()?;
        if x < 0
            || y < 0
            || width <= 0
            || height <= 0
            || x.checked_add(width)? > self.bounds.width()
            || y.checked_add(height)? > self.bounds.height()
        {
            return None;
        }
        let x = x as usize;
        let y = y as usize;
        let width = width as usize;
        let height = height as usize;
        let s = self.stride;
        match &self.storage {
            PrefixStorage::Dense(data) => Some(
                i64::from(data[(y + height) * s + x + width])
                    - i64::from(data[y * s + x + width])
                    - i64::from(data[(y + height) * s + x])
                    + i64::from(data[y * s + x]),
            ),
            PrefixStorage::Sparse(points) => {
                let mut count = 0_i64;
                for cy in y..y + height {
                    for cx in x..x + width {
                        count += i64::from(points.contains(&Point {
                            x: self.bounds.min_x + cx as i64,
                            y: self.bounds.min_y + cy as i64,
                        }));
                    }
                }
                Some(count)
            }
        }
    }
    pub fn occupied(&self, x: i64, y: i64, width: i64, height: i64) -> bool {
        self.count(x, y, width, height) != Some(0)
    }
    pub fn occupied_except(
        &self,
        x: i64,
        y: i64,
        width: i64,
        height: i64,
        excluded: impl IntoIterator<Item = Rectangle>,
    ) -> bool {
        let Some(mut count) = self.count(x, y, width, height) else {
            return true;
        };
        for p in excluded {
            let ix = (x + width).min(p.x + p.width) - x.max(p.x);
            let iy = (y + height).min(p.y + p.height) - y.max(p.y);
            if ix > 0 && iy > 0 {
                count -= ix * iy;
            }
        }
        count != 0
    }
}
pub fn placement_rectangle(p: &FacilityPlacement) -> Rectangle {
    Rectangle {
        x: p.x,
        y: p.y,
        width: p.width,
        height: p.height,
    }
}
pub fn resource(message: &str) -> Diagnostic {
    let mut e = Diagnostic::new(
        "factory.vocabulary-resource-incomplete",
        message,
        serde_json::Value::Null,
    );
    e.stage = "factory-pose-vocabulary";
    e
}
pub fn canvas(bounds: Bounds) -> Result<(i64, i64, i64)> {
    let width = bounds.width();
    let height = bounds.height();
    let total = width
        .checked_mul(height)
        .filter(|&n| n > 0)
        .ok_or_else(|| resource("Canvas scan size cannot be represented"))?;
    Ok((width, height, total))
}
pub fn modular_product(index: i64, stride: i64, total: i64) -> i64 {
    ((index as u128 * stride as u128) % total as u128) as i64
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
pub fn empty_rectangles(
    w: &Witness,
    bounds: Bounds,
    scan_work: u64,
    temp_bytes: usize,
) -> Result<Vec<Rectangle>> {
    let (_, _, area) = canvas(bounds)?;
    if area as u64 > scan_work {
        let mut e = resource("Empty-rectangle enumeration exceeded its configured scan work");
        e.entity = serde_json::json!({"required_cell_visits": area, "scan_work": scan_work});
        return Err(e);
    }
    let width = usize::try_from(bounds.width())
        .map_err(|_| resource("Rectangle scan width cannot be represented"))?;
    let row_bytes = width
        .checked_mul(std::mem::size_of::<i64>())
        .ok_or_else(|| resource("Rectangle scratch size overflow"))?;
    if row_bytes > temp_bytes {
        let mut e = resource("Rectangle scratch exceeded configured temporary memory");
        e.entity = serde_json::json!({"required_bytes": row_bytes, "temp_bytes": temp_bytes});
        return Err(e);
    }
    let mut occupied = IndexSet::new();
    for p in w
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
    {
        if occupied.len() == occupied.capacity() && !occupied.contains(&p) {
            occupied
                .try_reserve(1)
                .map_err(|_| resource("Rectangle occupancy allocation refused"))?;
        }
        occupied.insert(p);
    }
    let mut heights = Vec::new();
    heights
        .try_reserve_exact(width)
        .map_err(|_| resource("Rectangle row allocation refused"))?;
    heights.resize(width, 0_i64);
    let mut stack = Vec::<(usize, i64)>::new();
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
        stack.clear();
        for x in 0..=width {
            let h = if x == width { 0 } else { heights[x] };
            let mut left = x;
            while stack.last().is_some_and(|&(_, sh)| sh > h) {
                let (sl, sh) = stack.pop().unwrap();
                left = sl;
                if rectangles.len() == rectangles.capacity() {
                    let next = rectangles
                        .capacity()
                        .max(64)
                        .checked_mul(2)
                        .ok_or_else(|| resource("Rectangle candidate capacity overflow"))?;
                    let bytes = next
                        .checked_mul(std::mem::size_of::<Rectangle>())
                        .and_then(|n| {
                            stack
                                .capacity()
                                .checked_mul(std::mem::size_of::<(usize, i64)>())
                                .and_then(|stack_bytes| n.checked_add(stack_bytes))
                        })
                        .and_then(|n| n.checked_add(row_bytes))
                        .ok_or_else(|| resource("Rectangle candidate size overflow"))?;
                    if bytes > temp_bytes {
                        let mut e =
                            resource("Rectangle candidates exceeded configured temporary memory");
                        e.entity =
                            serde_json::json!({"required_bytes": bytes, "temp_bytes": temp_bytes});
                        return Err(e);
                    }
                    rectangles
                        .try_reserve_exact(next - rectangles.capacity())
                        .map_err(|_| resource("Rectangle candidate allocation refused"))?;
                }
                rectangles.push(Rectangle {
                    x: bounds.min_x + sl as i64,
                    y: y - sh + 1,
                    width: (x - sl) as i64,
                    height: sh,
                });
            }
            if h > 0 && stack.last().is_none_or(|&(_, sh)| sh < h) {
                if stack.len() == stack.capacity() {
                    let next = stack
                        .capacity()
                        .max(64)
                        .checked_mul(2)
                        .ok_or_else(|| resource("Rectangle stack capacity overflow"))?;
                    let bytes = next
                        .checked_mul(std::mem::size_of::<(usize, i64)>())
                        .and_then(|n| {
                            rectangles
                                .capacity()
                                .checked_mul(std::mem::size_of::<Rectangle>())
                                .and_then(|rect_bytes| n.checked_add(rect_bytes))
                        })
                        .and_then(|n| n.checked_add(row_bytes))
                        .ok_or_else(|| resource("Rectangle stack size overflow"))?;
                    if bytes > temp_bytes {
                        let mut e =
                            resource("Rectangle stack exceeded configured temporary memory");
                        e.entity =
                            serde_json::json!({"required_bytes": bytes, "temp_bytes": temp_bytes});
                        return Err(e);
                    }
                    stack
                        .try_reserve_exact(next - stack.capacity())
                        .map_err(|_| resource("Rectangle stack allocation refused"))?;
                }
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
    Ok(result)
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
