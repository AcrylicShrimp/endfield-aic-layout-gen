//! Compact rank/select view of legal placement poses for one immutable canvas.

use aic_layout::geometry::{Bounds, Point};
use serde::Serialize;

use super::PlacementDomain;
use crate::constructive::{Diagnostic, error};

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(super) struct PoseIndex {
    bounds: Bounds,
    stride: usize,
    width: usize,
    height: usize,
    occupancy_prefix: Vec<usize>,
    shapes: Vec<((i64, i64), ShapeRows)>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct ShapeRows {
    width: usize,
    height: usize,
    cumulative: Vec<usize>,
}

fn resource_error() -> Diagnostic {
    error(
        "learning-resource",
        "learning-domain",
        "placement index dimensions or host allocation exceed available resources",
    )
}

fn reserve_zeroed(length: usize) -> Result<Vec<usize>, Diagnostic> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(length)
        .map_err(|_| resource_error())?;
    values.resize(length, 0);
    Ok(values)
}

impl PoseIndex {
    pub(super) fn construction_upper_bytes(
        bounds: Bounds,
        rotation_count: usize,
    ) -> Result<usize, Diagnostic> {
        let width = usize::try_from(bounds.width()).map_err(|_| resource_error())?;
        let height = usize::try_from(bounds.height()).map_err(|_| resource_error())?;
        let prefix = width
            .checked_add(1)
            .and_then(|width| {
                height
                    .checked_add(1)
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|cells| cells.checked_mul(std::mem::size_of::<usize>()))
            .ok_or_else(resource_error)?;
        let per_shape = std::mem::size_of::<(i64, i64)>()
            .checked_add(std::mem::size_of::<((i64, i64), ShapeRows)>())
            .and_then(|bytes| {
                height
                    .checked_mul(std::mem::size_of::<usize>())
                    .and_then(|rows| bytes.checked_add(rows))
            })
            .ok_or_else(resource_error)?;
        prefix
            .checked_add(
                rotation_count
                    .checked_mul(per_shape)
                    .ok_or_else(resource_error)?,
            )
            .ok_or_else(resource_error)
    }

    pub(super) fn heap_bytes(&self) -> usize {
        self.occupancy_prefix
            .capacity()
            .saturating_mul(std::mem::size_of::<usize>())
            .saturating_add(
                self.shapes
                    .capacity()
                    .saturating_mul(std::mem::size_of::<((i64, i64), ShapeRows)>()),
            )
            .saturating_add(self.shapes.iter().fold(0usize, |total, (_, rows)| {
                total.saturating_add(
                    rows.cumulative
                        .capacity()
                        .saturating_mul(std::mem::size_of::<usize>()),
                )
            }))
    }

    pub(super) fn new(
        bounds: Bounds,
        blocked: &[Point],
        placements: &[PlacementDomain],
    ) -> Result<Self, Diagnostic> {
        let width = usize::try_from(bounds.width()).map_err(|_| resource_error())?;
        let height = usize::try_from(bounds.height()).map_err(|_| resource_error())?;
        let stride = width.checked_add(1).ok_or_else(resource_error)?;
        let length = stride
            .checked_mul(height.checked_add(1).ok_or_else(resource_error)?)
            .ok_or_else(resource_error)?;
        let mut occupancy_prefix = reserve_zeroed(length)?;
        for point in blocked {
            if point.x < bounds.min_x
                || point.x > bounds.max_x
                || point.y < bounds.min_y
                || point.y > bounds.max_y
            {
                continue;
            }
            let x = usize::try_from(point.x - bounds.min_x).map_err(|_| resource_error())?;
            let y = usize::try_from(point.y - bounds.min_y).map_err(|_| resource_error())?;
            occupancy_prefix[(y + 1) * stride + x + 1] = 1;
        }
        for y in 1..=height {
            for x in 1..=width {
                let at = y * stride + x;
                occupancy_prefix[at] += occupancy_prefix[at - 1] + occupancy_prefix[at - stride]
                    - occupancy_prefix[at - stride - 1];
            }
        }
        let shape_count = placements.iter().try_fold(0usize, |total, domain| {
            total
                .checked_add(domain.rotations.len())
                .ok_or_else(resource_error)
        })?;
        let mut shape_keys = Vec::new();
        shape_keys
            .try_reserve_exact(shape_count)
            .map_err(|_| resource_error())?;
        for domain in placements {
            shape_keys.extend(
                domain
                    .rotations
                    .iter()
                    .map(|&rotation| rotated_shape(domain, rotation)),
            );
        }
        shape_keys.sort_unstable();
        shape_keys.dedup();
        let mut result = Self {
            bounds,
            stride,
            width,
            height,
            occupancy_prefix,
            shapes: Vec::new(),
        };
        result
            .shapes
            .try_reserve_exact(shape_keys.len())
            .map_err(|_| resource_error())?;
        for shape in shape_keys {
            let rows = result.build_shape(shape)?;
            result.shapes.push((shape, rows));
        }
        Ok(result)
    }

    fn build_shape(&self, shape: (i64, i64)) -> Result<ShapeRows, Diagnostic> {
        let width = usize::try_from(shape.0).map_err(|_| resource_error())?;
        let height = usize::try_from(shape.1).map_err(|_| resource_error())?;
        if width == 0 || height == 0 || width > self.width || height > self.height {
            return Ok(ShapeRows {
                width,
                height,
                cumulative: Vec::new(),
            });
        }
        let rows = self.height - height + 1;
        let mut cumulative = Vec::new();
        cumulative
            .try_reserve_exact(rows)
            .map_err(|_| resource_error())?;
        let mut total = 0usize;
        for y in 0..rows {
            for x in 0..=self.width - width {
                total = total
                    .checked_add(usize::from(self.clear(x, y, width, height)))
                    .ok_or_else(resource_error)?;
            }
            cumulative.push(total);
        }
        Ok(ShapeRows {
            width,
            height,
            cumulative,
        })
    }

    fn clear(&self, x: usize, y: usize, width: usize, height: usize) -> bool {
        let right = x + width;
        let bottom = y + height;
        let prefix = &self.occupancy_prefix;
        let count = prefix[bottom * self.stride + right] + prefix[y * self.stride + x]
            - prefix[y * self.stride + right]
            - prefix[bottom * self.stride + x];
        count == 0
    }

    pub(super) fn pose_count(&self, domain: &PlacementDomain) -> usize {
        domain.rotations.iter().fold(0usize, |total, &rotation| {
            let shape = rotated_shape(domain, rotation);
            total.saturating_add(self.shape_rows(shape).total())
        })
    }

    fn shape_rows(&self, shape: (i64, i64)) -> &ShapeRows {
        let index = self
            .shapes
            .binary_search_by_key(&shape, |entry| entry.0)
            .expect("indexed placement shape exists");
        &self.shapes[index].1
    }

    pub(super) fn pose_at(
        &self,
        domain: &PlacementDomain,
        mut rank: usize,
    ) -> Option<(Point, i64)> {
        for &rotation in &domain.rotations {
            let shape = rotated_shape(domain, rotation);
            let rows = self.shape_rows(shape);
            if rank >= rows.total() {
                rank -= rows.total();
                continue;
            }
            let y = rows.cumulative.partition_point(|&count| count <= rank);
            let prior = if y == 0 { 0 } else { rows.cumulative[y - 1] };
            let mut within_row = rank - prior;
            for x in 0..=self.width - rows.width {
                if self.clear(x, y, rows.width, rows.height) {
                    if within_row == 0 {
                        return Some((
                            Point {
                                x: self.bounds.min_x + x as i64,
                                y: self.bounds.min_y + y as i64,
                            },
                            rotation,
                        ));
                    }
                    within_row -= 1;
                }
            }
            unreachable!("indexed row count and occupancy disagree");
        }
        None
    }
}

impl ShapeRows {
    fn total(&self) -> usize {
        self.cumulative.last().copied().unwrap_or(0)
    }
}

fn rotated_shape(domain: &PlacementDomain, rotation: i64) -> (i64, i64) {
    if rotation.rem_euclid(180) == 0 {
        (domain.width, domain.height)
    } else {
        (domain.height, domain.width)
    }
}

#[cfg(test)]
#[test]
fn indexed_poses_match_rotation_major_scan_with_blocked_holes() {
    let bounds = Bounds {
        min_x: 2,
        min_y: 5,
        max_x: 10,
        max_y: 11,
    };
    let blocked = [Point { x: 4, y: 7 }, Point { x: 8, y: 9 }];
    let placements = [
        PlacementDomain {
            block: 0,
            id: "rectangle".into(),
            width: 3,
            height: 2,
            rotations: vec![0, 90, 180, 270],
        },
        PlacementDomain {
            block: 1,
            id: "square".into(),
            width: 2,
            height: 2,
            rotations: vec![0, 90],
        },
    ];
    let index = PoseIndex::new(bounds, &blocked, &placements).unwrap();
    for placement in &placements {
        let mut expected = Vec::new();
        for &rotation in &placement.rotations {
            let (width, height) = rotated_shape(placement, rotation);
            for y in bounds.min_y..=bounds.max_y - height + 1 {
                for x in bounds.min_x..=bounds.max_x - width + 1 {
                    let clear = (y..y + height).all(|cell_y| {
                        (x..x + width).all(|cell_x| {
                            !blocked.contains(&Point {
                                x: cell_x,
                                y: cell_y,
                            })
                        })
                    });
                    if clear {
                        expected.push((Point { x, y }, rotation));
                    }
                }
            }
        }
        assert_eq!(index.pose_count(placement), expected.len());
        assert_eq!(
            (0..expected.len())
                .map(|rank| index.pose_at(placement, rank).unwrap())
                .collect::<Vec<_>>(),
            expected
        );
    }
}
