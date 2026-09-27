//! Sequence-pair longest-path packing with independently sampled nonnegative gaps.
use crate::config::{Config, failure};
use aic_layout::{
    geometry::{Diagnostic, Point},
    physical::{Block, Pose},
    projection::{Port, rotate},
    rng::Rng,
    validation::Catalogs,
};

pub(crate) struct Packing {
    pub blocks: Vec<Block>,
    pub poses: Vec<Pose>,
    pub ports: Vec<Vec<Port>>,
    pub sizes: Vec<(i64, i64)>,
    pub width: i64,
    pub height: i64,
    pub first: Vec<usize>,
    pub second: Vec<usize>,
}

pub(crate) fn sample(
    catalogs: &Catalogs,
    config: &Config,
    n: usize,
    rng: &mut Rng,
) -> Result<Packing, Diagnostic> {
    let pool: Vec<_> = catalogs
        .facilities
        .catalog()
        .facilities
        .iter()
        .filter(|f| {
            (config.facility_ids.is_empty() || config.facility_ids.contains(&f.id))
                && f.ports
                    .iter()
                    .filter(|p| config.transports.contains(&p.transport))
                    .count()
                    >= 2
        })
        .collect();
    if pool.is_empty() {
        return Err(failure(
            "catalog-domain",
            "facilities",
            "No block has two compatible ports",
        ));
    }
    let mut blocks = Vec::with_capacity(n);
    let mut poses = Vec::with_capacity(n);
    let mut sizes = Vec::with_capacity(n);
    for i in 0..n {
        let def = pool[rng.index(pool.len())];
        let rotation = def.allowed_rotations[rng.index(def.allowed_rotations.len())];
        let (w, h, _) = rotate(def, rotation, Point::default());
        if w > 1000 || h > 1000 || w * h > 10_000 {
            return Err(failure(
                "body-guard",
                def.id.clone(),
                "Selected footprint exceeds the materialization guard",
            ));
        }
        let id = format!("block-{i}");
        blocks.push(Block {
            id: id.clone(),
            facility: def.id.clone(),
        });
        poses.push(Pose {
            block: id,
            position: Point {
                x: config.margin,
                y: config.margin,
            },
            rotation,
        });
        sizes.push((w, h));
    }
    let mut first: Vec<_> = (0..n).collect();
    let mut second = first.clone();
    rng.shuffle(&mut first);
    rng.shuffle(&mut second);
    let mut rank = vec![0; n];
    for (i, &id) in second.iter().enumerate() {
        rank[id] = i;
    }
    for (at, &i) in first.iter().enumerate() {
        for &j in &first[at + 1..] {
            let gap =
                config.gap[0] + rng.index((config.gap[1] - config.gap[0] + 1) as usize) as i64;
            if rank[i] < rank[j] {
                poses[j].position.x = poses[j]
                    .position
                    .x
                    .max(poses[i].position.x + sizes[i].0 + gap);
            } else {
                poses[j].position.y = poses[j]
                    .position
                    .y
                    .max(poses[i].position.y + sizes[i].1 + gap);
            }
        }
    }
    let width = poses
        .iter()
        .zip(&sizes)
        .map(|(p, s)| p.position.x + s.0)
        .max()
        .unwrap()
        + config.margin;
    let height = poses
        .iter()
        .zip(&sizes)
        .map(|(p, s)| p.position.y + s.1)
        .max()
        .unwrap()
        + config.margin;
    if width
        .checked_mul(height)
        .is_none_or(|area| area > config.max_cells as i64)
    {
        return Err(failure(
            "canvas-guard",
            "packing",
            format!(
                "Sampled {width}x{height} canvas exceeds {} cells",
                config.max_cells
            ),
        ));
    }
    let ports = poses
        .iter()
        .zip(&blocks)
        .map(|(p, b)| {
            rotate(
                catalogs.facilities.facility(&b.facility).unwrap(),
                p.rotation,
                p.position,
            )
            .2
        })
        .collect();
    Ok(Packing {
        blocks,
        poses,
        ports,
        sizes,
        width,
        height,
        first,
        second,
    })
}
