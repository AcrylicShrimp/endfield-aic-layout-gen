//! Native pilot environment and the isolated observation experiment.

mod acceptance;
mod catalog;
mod diagnostic;
mod engine;
mod python_engine;
mod state;

use std::collections::{HashMap, HashSet};

use numpy::ndarray::{Array2, Array3};
use numpy::{IntoPyArray, PyArray2, PyArray3, PyReadonlyArray2};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

type Point = (i64, i64);
type PortOffsets = HashMap<(String, String, i64), (Point, Point)>;
type TerminalSpec = (String, usize, Option<usize>, bool);
type Encoded<'py> = (Bound<'py, PyArray3<f32>>, Bound<'py, PyArray2<f32>>, Point);

fn invalid(message: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("native.input: {message}"))
}

fn add(a: i64, b: i64) -> PyResult<i64> {
    a.checked_add(b)
        .ok_or_else(|| invalid("coordinate overflow"))
}

fn get<'py>(dict: &Bound<'py, PyDict>, key: &str) -> PyResult<Bound<'py, PyAny>> {
    dict.get_item(key)?
        .ok_or_else(|| invalid(format!("missing {key}")))
}

fn direction(arm: Point) -> PyResult<usize> {
    match arm {
        (0, -1) => Ok(0),
        (1, 0) => Ok(1),
        (0, 1) => Ok(2),
        (-1, 0) => Ok(3),
        _ => Err(invalid("non-cardinal arm")),
    }
}

fn side(value: &str) -> PyResult<Point> {
    match value {
        "N" => Ok((0, -1)),
        "E" => Ok((1, 0)),
        "S" => Ok((0, 1)),
        "W" => Ok((-1, 0)),
        _ => Err(invalid(format!("unknown side {value}"))),
    }
}

struct Network {
    cells: HashSet<Point>,
    edges: HashSet<(Point, Point)>,
}

struct Snapshot {
    poses: Vec<(i64, i64, i64)>,
    bodies: Vec<Vec<Point>>,
    networks: Vec<Network>,
    terminals: Vec<Option<(Point, Point)>>,
}

#[pyclass(frozen)]
struct GeometryEncoder {
    fids: Vec<String>,
    nids: Vec<String>,
    sizes: Vec<(i64, i64)>,
    terminals: Vec<TerminalSpec>,
    port_offsets: PortOffsets,
    template: Array2<f32>,
    scale: f64,
}

impl GeometryEncoder {
    fn snapshot(&self, state: &Bound<'_, PyDict>, guard: usize) -> PyResult<Snapshot> {
        let placements = get(state, "placements")?.cast_into::<PyDict>()?;
        let networks = get(state, "networks")?.cast_into::<PyDict>()?;
        let bindings = get(state, "bindings")?.cast_into::<PyDict>()?;
        let mut poses = Vec::new();
        let mut bodies = Vec::new();
        for (fid, &(mut w, mut h)) in self.fids.iter().zip(&self.sizes) {
            let pose: Vec<i64> = get(&placements, fid)?.extract()?;
            if pose.len() != 3 || ![0, 90, 180, 270].contains(&pose[2]) {
                return Err(invalid(format!("invalid pose for {fid}")));
            }
            if pose[2] == 90 || pose[2] == 270 {
                std::mem::swap(&mut w, &mut h);
            }
            if (w as i128) * (h as i128) > guard as i128 {
                return Err(PyValueError::new_err(
                    "observation.resource_guard: facility exceeds raster guard",
                ));
            }
            let mut body = Vec::new();
            for x in 0..w {
                for y in 0..h {
                    body.push((add(pose[0], x)?, add(pose[1], y)?));
                }
            }
            poses.push((pose[0], pose[1], pose[2]));
            bodies.push(body);
        }
        let mut nets = Vec::new();
        for nid in &self.nids {
            let network = get(&networks, nid)?.cast_into::<PyDict>()?;
            nets.push(Network {
                cells: get(&network, "cells")?.extract()?,
                edges: get(&network, "edges")?.extract()?,
            });
        }
        let mut terminals = Vec::new();
        for (tid, _, facility, _) in &self.terminals {
            let binding = get(&bindings, tid)?;
            if binding.is_none() {
                terminals.push(None);
                continue;
            }
            let binding = binding.cast_into::<PyDict>()?;
            let location = if let Some(fi) = facility {
                let port: String = get(&binding, "port")?.extract()?;
                let (x, y, rotation) = poses[*fi];
                let &(offset, arm) = self
                    .port_offsets
                    .get(&(self.fids[*fi].clone(), port, rotation))
                    .ok_or_else(|| invalid(format!("unknown port offset for {tid}")))?;
                ((add(x, offset.0)?, add(y, offset.1)?), arm)
            } else {
                let cell: Vec<i64> = get(&binding, "cell")?.extract()?;
                if cell.len() != 2 {
                    return Err(invalid(format!("invalid cell for {tid}")));
                }
                let edge: String = get(&binding, "side")?.extract()?;
                ((cell[0], cell[1]), side(&edge)?)
            };
            terminals.push(Some(location));
        }
        Ok(Snapshot {
            poses,
            bodies,
            networks: nets,
            terminals,
        })
    }
}

#[pymethods]
impl GeometryEncoder {
    #[new]
    fn new(
        fids: Vec<String>,
        nids: Vec<String>,
        sizes: Vec<(i64, i64)>,
        terminals: Vec<TerminalSpec>,
        port_offsets: PortOffsets,
        template: PyReadonlyArray2<'_, f32>,
        scale: f64,
    ) -> PyResult<Self> {
        if fids.is_empty()
            || sizes.len() != fids.len()
            || sizes.iter().any(|&(w, h)| w <= 0 || h <= 0)
            || !scale.is_finite()
            || scale <= 0.0
            || terminals
                .iter()
                .any(|(_, n, f, _)| *n >= nids.len() || f.is_some_and(|f| f >= fids.len()))
        {
            return Err(invalid("invalid catalog geometry metadata"));
        }
        let template = template.as_array().to_owned();
        let dynamic = nids.len() + fids.len() + 2;
        if template.shape() != [terminals.len() * 2, dynamic + 17] {
            return Err(invalid("invalid object template shape"));
        }
        Ok(Self {
            fids,
            nids,
            sizes,
            terminals,
            port_offsets,
            template,
            scale,
        })
    }

    fn encode<'py>(
        &self,
        py: Python<'py>,
        draft: &Bound<'py, PyDict>,
        base: &Bound<'py, PyDict>,
        halo: i64,
        guard: usize,
    ) -> PyResult<Encoded<'py>> {
        if halo < 0 || guard == 0 {
            return Err(invalid("halo must be nonnegative and guard positive"));
        }
        let states = [self.snapshot(draft, guard)?, self.snapshot(base, guard)?];
        let points = states.iter().flat_map(|s| {
            s.bodies
                .iter()
                .flatten()
                .chain(s.networks.iter().flat_map(|n| n.cells.iter()))
        });
        let mut low = (i64::MAX, i64::MAX);
        let mut high = (i64::MIN, i64::MIN);
        for &(x, y) in points {
            low = (low.0.min(x), low.1.min(y));
            high = (high.0.max(x), high.1.max(y));
        }
        let origin = (add(low.0, -halo)?, add(low.1, -halo)?);
        let high = (add(high.0, halo)?, add(high.1, halo)?);
        let w = high.0 as i128 - origin.0 as i128 + 1;
        let h = high.1 as i128 - origin.1 as i128 + 1;
        if w <= 0 || h <= 0 || w.checked_mul(h).is_none_or(|cells| cells > guard as i128) {
            return Err(PyValueError::new_err(format!(
                "observation.resource_guard: {w}x{h} exceeds {guard}"
            )));
        }
        let (w, h) = (
            usize::try_from(w).map_err(invalid)?,
            usize::try_from(h).map_err(invalid)?,
        );
        let per_state = self.fids.len() + self.nids.len() + 8 + self.terminals.len();
        let channels = per_state
            .checked_mul(2)
            .and_then(|v| v.checked_add(3))
            .ok_or_else(|| invalid("channel overflow"))?;
        let length = channels
            .checked_mul(w)
            .and_then(|v| v.checked_mul(h))
            .filter(|v| *v <= isize::MAX as usize / size_of::<f32>())
            .ok_or_else(|| invalid("allocation size overflow"))?;
        let mut data = Vec::new();
        data.try_reserve_exact(length).map_err(invalid)?;
        data.resize(length, 0.0f32);
        let mut raster = Array3::from_shape_vec((channels, h, w), data).map_err(invalid)?;
        let mut objects = self.template.clone();
        let dynamic = self.nids.len() + self.fids.len() + 2;
        let pixel = |point: Point| -> PyResult<(usize, usize)> {
            let x = point.0 as i128 - origin.0 as i128;
            let y = point.1 as i128 - origin.1 as i128;
            if x < 0 || y < 0 || x >= w as i128 || y >= h as i128 {
                return Err(invalid("geometry outside occupied bounds"));
            }
            Ok((y as usize, x as usize))
        };
        for (state_index, state) in states.iter().enumerate() {
            let offset = state_index * per_state;
            for (fi, body) in state.bodies.iter().enumerate() {
                for &point in body {
                    let (y, x) = pixel(point)?;
                    raster[[offset + fi, y, x]] = 1.0;
                }
            }
            let net_offset = offset + self.fids.len();
            let arm_offset = net_offset + self.nids.len();
            for (ni, net) in state.networks.iter().enumerate() {
                for &point in &net.cells {
                    let (y, x) = pixel(point)?;
                    raster[[net_offset + ni, y, x]] = 1.0;
                }
                for &(a, b) in &net.edges {
                    if !net.cells.contains(&a) || !net.cells.contains(&b) {
                        return Err(invalid("edge has absent endpoint"));
                    }
                    let arm = (
                        b.0.checked_sub(a.0)
                            .ok_or_else(|| invalid("edge overflow"))?,
                        b.1.checked_sub(a.1)
                            .ok_or_else(|| invalid("edge overflow"))?,
                    );
                    let forward = direction(arm)?;
                    let (y, x) = pixel(a)?;
                    raster[[arm_offset + 4 + forward, y, x]] = 1.0;
                    let (y, x) = pixel(b)?;
                    raster[[arm_offset + (forward + 2) % 4, y, x]] = 1.0;
                }
            }
            for (ti, ((_, ni, facility, source), location)) in
                self.terminals.iter().zip(&state.terminals).enumerate()
            {
                let row = state_index * self.terminals.len() + ti;
                let (position, arm) = location.unwrap_or(((0, 0), (0, 0)));
                let bound = location.is_some();
                if bound {
                    let (y, x) = pixel(position)?;
                    raster[[arm_offset + 8 + ti, y, x]] = 1.0;
                    if !state.networks[*ni].cells.contains(&position) {
                        return Err(invalid("bound terminal cell absent"));
                    }
                    let d = direction(arm)?;
                    raster[[arm_offset + usize::from(!source) * 4 + d, y, x]] = 1.0;
                    objects[[row, dynamic + 3 + d]] = 1.0;
                }
                objects[[row, dynamic]] = if bound { 1.0 } else { 0.0 };
                objects[[row, dynamic + 1]] = (position.0 as f64 / self.scale) as f32;
                objects[[row, dynamic + 2]] = (position.1 as f64 / self.scale) as f32;
                if let Some(fi) = facility {
                    let (x, y, r) = state.poses[*fi];
                    objects[[row, dynamic + 11]] = (x as f64 / self.scale) as f32;
                    objects[[row, dynamic + 12]] = (y as f64 / self.scale) as f32;
                    objects[[row, dynamic + 13 + (r / 90) as usize]] = 1.0;
                }
            }
        }
        Ok((raster.into_pyarray(py), objects.into_pyarray(py), origin))
    }
}

#[pymodule]
fn aic_pilot_native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<python_engine::PythonEngine>()?;
    module.add_class::<GeometryEncoder>()?;
    Ok(())
}
