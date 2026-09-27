//! Seeded sequence-pair placement for a caller-supplied, frozen block set.

use std::collections::BTreeSet;

use aic_layout::{
    geometry::{Diagnostic, Point},
    physical::{Block, Pose},
    projection::rotate,
    rng::Rng,
    validation::Catalogs,
};
use serde::{Deserialize, Serialize};

pub const PACKING_DENSITY_POLICY_VERSION: &str = "recipe-packing-density-v3";
pub const COMPACT_DENSITY: DensityBand = DensityBand {
    minimum: 0.12,
    maximum: 0.30,
};
pub const BALANCED_DENSITY: DensityBand = DensityBand {
    minimum: 0.06,
    maximum: 0.12,
};
pub const SPACIOUS_DENSITY: DensityBand = DensityBand {
    minimum: 0.02,
    maximum: 0.06,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementRequest {
    pub seed: u64,
    pub gap: [i64; 2],
    pub margin: i64,
    pub max_cells: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Placement {
    pub poses: Vec<Pose>,
    pub sequence_pair: [Vec<usize>; 2],
    pub body_width: i64,
    pub body_height: i64,
    pub body_cells: u64,
    pub body_bounding_area: u64,
    pub body_density: f64,
    pub gap: [i64; 2],
    pub canvas_width: i64,
    pub canvas_height: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DensityBand {
    pub minimum: f64,
    pub maximum: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DensityPlacementRequest {
    pub seed: u64,
    pub band: DensityBand,
    pub max_uniform_gap: i64,
    pub margin: i64,
    pub max_cells: usize,
}

/// Place exactly the supplied block instances without changing their identities
/// or facility definitions.
pub fn sample(
    catalogs: &Catalogs,
    blocks: &[Block],
    request: &PlacementRequest,
) -> Result<Placement, Diagnostic> {
    validate_request(catalogs, blocks, request)?;

    let mut rng = Rng::new(request.seed);
    let mut poses = Vec::with_capacity(blocks.len());
    let mut sizes = Vec::with_capacity(blocks.len());
    let mut body_cells = 0_u64;
    for block in blocks {
        let definition = catalogs
            .facilities
            .facility(&block.facility)
            .expect("facility existence was validated");
        let rotation = definition.allowed_rotations[rng.index(definition.allowed_rotations.len())];
        let (width, height, _) = rotate(definition, rotation, Point::default());
        body_cells = body_cells
            .checked_add(
                width
                    .checked_mul(height)
                    .and_then(|area| u64::try_from(area).ok())
                    .ok_or_else(|| {
                        failure(
                            "body-guard",
                            block.id.clone(),
                            "facility body area overflow",
                        )
                    })?,
            )
            .ok_or_else(|| {
                failure(
                    "body-guard",
                    block.id.clone(),
                    "facility body area overflow",
                )
            })?;
        poses.push(Pose {
            block: block.id.clone(),
            position: Point {
                x: request.margin,
                y: request.margin,
            },
            rotation,
        });
        sizes.push((width, height));
    }

    let mut first: Vec<_> = (0..blocks.len()).collect();
    let mut second = first.clone();
    rng.shuffle(&mut first);
    rng.shuffle(&mut second);
    let mut second_rank = vec![0; blocks.len()];
    for (rank, &block) in second.iter().enumerate() {
        second_rank[block] = rank;
    }
    for (at, &left) in first.iter().enumerate() {
        for &right in &first[at + 1..] {
            let gap =
                request.gap[0] + rng.index((request.gap[1] - request.gap[0] + 1) as usize) as i64;
            if second_rank[left] < second_rank[right] {
                poses[right].position.x = poses[right]
                    .position
                    .x
                    .max(poses[left].position.x + sizes[left].0 + gap);
            } else {
                poses[right].position.y = poses[right]
                    .position
                    .y
                    .max(poses[left].position.y + sizes[left].1 + gap);
            }
        }
    }

    let min_x = poses.iter().map(|pose| pose.position.x).min().unwrap();
    let min_y = poses.iter().map(|pose| pose.position.y).min().unwrap();
    let max_x = poses
        .iter()
        .zip(&sizes)
        .map(|(pose, size)| pose.position.x + size.0)
        .max()
        .unwrap();
    let max_y = poses
        .iter()
        .zip(&sizes)
        .map(|(pose, size)| pose.position.y + size.1)
        .max()
        .unwrap();
    let body_width = max_x - min_x;
    let body_height = max_y - min_y;
    let body_bounding_area = body_width
        .checked_mul(body_height)
        .and_then(|area| u64::try_from(area).ok())
        .ok_or_else(|| failure("canvas-guard", "placement", "body bounding area overflow"))?;
    let canvas_width = max_x + request.margin;
    let canvas_height = max_y + request.margin;
    if canvas_width
        .checked_mul(canvas_height)
        .is_none_or(|area| area > request.max_cells as i64)
    {
        return Err(failure(
            "canvas-guard",
            "placement",
            format!(
                "sampled {canvas_width}x{canvas_height} canvas exceeds {} cells",
                request.max_cells
            ),
        ));
    }

    Ok(Placement {
        poses,
        sequence_pair: [first, second],
        body_width,
        body_height,
        body_cells,
        body_bounding_area,
        body_density: body_cells as f64 / body_bounding_area as f64,
        gap: request.gap,
        canvas_width,
        canvas_height,
    })
}

/// Keep one seeded rotation/sequence-pair draw fixed, find every uniform integer
/// gap inside the requested band, and select reproducibly within that interval.
/// Density decreases monotonically as the uniform gap grows. Gap zero is excluded
/// because facility bodies could cover every outward port connection cell.
pub fn sample_in_density_band(
    catalogs: &Catalogs,
    blocks: &[Block],
    request: &DensityPlacementRequest,
) -> Result<Placement, Diagnostic> {
    if !request.band.minimum.is_finite()
        || !request.band.maximum.is_finite()
        || request.band.minimum <= 0.0
        || request.band.maximum <= request.band.minimum
        || request.band.maximum > 1.0
        || !(0..=128).contains(&request.max_uniform_gap)
    {
        return Err(failure(
            "placement-request",
            "density-band",
            "density band must satisfy 0 < minimum < maximum <= 1 and max gap must be 0..=128",
        ));
    }
    let at_gap = |gap| {
        sample(
            catalogs,
            blocks,
            &PlacementRequest {
                seed: request.seed,
                gap: [gap, gap],
                margin: request.margin,
                max_cells: request.max_cells,
            },
        )
    };
    if request.max_uniform_gap < 1 {
        return Err(failure(
            "placement-request",
            "density-band",
            "density-controlled placement requires at least one port-escape cell",
        ));
    }
    let one = at_gap(1)?;
    if one.body_density < request.band.minimum {
        return Err(packing_target_missed(one.body_density, request.band));
    }
    let first = if one.body_density < request.band.maximum {
        1
    } else {
        first_gap_below(1, request.max_uniform_gap, request.band.maximum, &at_gap)?
            .ok_or_else(|| packing_target_missed(one.body_density, request.band))?
    };
    let last = first_gap_below(
        first,
        request.max_uniform_gap,
        request.band.minimum,
        &at_gap,
    )?
    .map_or(request.max_uniform_gap, |gap| gap - 1);
    if first > last {
        return Err(packing_target_missed(one.body_density, request.band));
    }
    let span = u64::try_from(last - first + 1).expect("positive bounded gap span");
    let mixed = request
        .seed
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .rotate_left(17)
        ^ 0xd1b5_4a32_d192_ed03;
    let selected = first + i64::try_from(mixed % span).expect("gap span fits i64");
    let placement = at_gap(selected)?;
    debug_assert!(in_band(placement.body_density, request.band));
    Ok(placement)
}

fn first_gap_below(
    lower: i64,
    upper: i64,
    threshold: f64,
    at_gap: &impl Fn(i64) -> Result<Placement, Diagnostic>,
) -> Result<Option<i64>, Diagnostic> {
    let below = |gap| match at_gap(gap) {
        Ok(placement) => Ok(placement.body_density < threshold),
        Err(diagnostic) if diagnostic.code == "aic-layout-gen.canvas-guard" => Ok(true),
        Err(diagnostic) => Err(diagnostic),
    };
    let mut outside = lower;
    let mut inside = lower;
    loop {
        if inside == upper {
            return Ok(None);
        }
        inside = (inside.saturating_mul(2)).max(inside + 1).min(upper);
        if below(inside)? {
            break;
        }
        outside = inside;
    }
    while outside + 1 < inside {
        let middle = outside + (inside - outside) / 2;
        if below(middle)? {
            inside = middle;
        } else {
            outside = middle;
        }
    }
    Ok(Some(inside))
}

fn in_band(density: f64, band: DensityBand) -> bool {
    density >= band.minimum && (density < band.maximum || band.maximum == 1.0 && density <= 1.0)
}

fn packing_target_missed(density: f64, band: DensityBand) -> Diagnostic {
    failure(
        "packing-target-missed",
        "density",
        format!(
            "achieved body density {density:.6} is outside [{:.6}, {:.6})",
            band.minimum, band.maximum
        ),
    )
}

fn validate_request(
    catalogs: &Catalogs,
    blocks: &[Block],
    request: &PlacementRequest,
) -> Result<(), Diagnostic> {
    if blocks.is_empty()
        || blocks.len() > 4096
        || request.gap[0] < 0
        || request.gap[1] < request.gap[0]
        || request.gap[1] > 128
        || !(0..=128).contains(&request.margin)
        || !(16..=250_000_000).contains(&request.max_cells)
    {
        return Err(failure(
            "placement-request",
            "request",
            "invalid block count, gap, margin or canvas limit",
        ));
    }
    let mut ids = BTreeSet::new();
    for block in blocks {
        let Some(definition) = catalogs.facilities.facility(&block.facility) else {
            return Err(failure(
                "placement-block",
                block.id.clone(),
                "block references an unknown facility",
            ));
        };
        if !ids.insert(block.id.as_str()) || definition.allowed_rotations.is_empty() {
            return Err(failure(
                "placement-block",
                block.id.clone(),
                "block identity is duplicated or facility has no legal rotation",
            ));
        }
        let footprint = &definition.footprint;
        if footprint.width > 1000
            || footprint.height > 1000
            || footprint.width * footprint.height > 10_000
        {
            return Err(failure(
                "body-guard",
                block.id.clone(),
                "facility footprint exceeds the placement guard",
            ));
        }
    }
    Ok(())
}

fn failure(
    code: &str,
    entity: impl Into<serde_json::Value>,
    message: impl Into<String>,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(format!("aic-layout-gen.{code}"), message, entity);
    diagnostic.stage = "sequence-pair-placement";
    diagnostic
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io;
    use std::path::Path;

    #[test]
    fn sparse_placement_admits_canvas_above_legacy_generator_guard() {
        let loaded = io::load_catalogs(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized"),
        )
        .unwrap();
        let blocks = (0..64)
            .map(|index| Block {
                id: format!("facility-{index}"),
                facility: "component-mc-1-mode-normal".into(),
            })
            .collect::<Vec<_>>();
        let placement = sample(
            &loaded.catalogs,
            &blocks,
            &PlacementRequest {
                seed: 17,
                gap: [128, 128],
                margin: 4,
                max_cells: 250_000_000,
            },
        )
        .unwrap();
        assert!(placement.canvas_width * placement.canvas_height > 250_000);
        assert_eq!(placement.poses.len(), blocks.len());
    }

    #[test]
    fn preserves_frozen_blocks_and_reports_body_density() {
        let loaded = io::load_catalogs(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized"),
        )
        .unwrap();
        let blocks = vec![
            Block {
                id: "facility-a".into(),
                facility: "component-mc-1-mode-normal".into(),
            },
            Block {
                id: "facility-b".into(),
                facility: "dismantler-1-mode-liquid".into(),
            },
            Block {
                id: "facility-c".into(),
                facility: "component-mc-1-mode-normal".into(),
            },
        ];
        let request = PlacementRequest {
            seed: 17,
            gap: [0, 3],
            margin: 4,
            max_cells: 65_536,
        };

        let placement = sample(&loaded.catalogs, &blocks, &request).unwrap();

        assert_eq!(
            placement
                .poses
                .iter()
                .map(|pose| pose.block.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["facility-a", "facility-b", "facility-c"])
        );
        assert!(placement.body_density > 0.0 && placement.body_density <= 1.0);
        assert_eq!(
            placement.body_bounding_area,
            (placement.body_width * placement.body_height) as u64
        );
        assert_eq!(
            sample(&loaded.catalogs, &blocks, &request).unwrap(),
            placement
        );
    }

    #[test]
    fn selects_a_uniform_gap_from_measured_density() {
        let loaded = io::load_catalogs(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized"),
        )
        .unwrap();
        let blocks = (0..16)
            .map(|index| Block {
                id: format!("facility-{index}"),
                facility: "component-mc-1-mode-normal".into(),
            })
            .collect::<Vec<_>>();

        let placement = sample_in_density_band(
            &loaded.catalogs,
            &blocks,
            &DensityPlacementRequest {
                seed: 29,
                band: DensityBand {
                    minimum: 0.13,
                    maximum: 0.30,
                },
                max_uniform_gap: 128,
                margin: 4,
                max_cells: 65_536,
            },
        )
        .unwrap();

        assert!(placement.body_density >= 0.13 && placement.body_density < 0.30);
        assert_eq!(placement.gap[0], placement.gap[1]);
    }
}
