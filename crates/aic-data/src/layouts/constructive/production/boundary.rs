//! Candidate-local occupancy and commodity-local exterior reachability.
use super::*;
mod parallel;

const FACILITY: u8 = 1;
const BELT: u8 = 2;
const PIPE: u8 = 4;
const HORIZONTAL: u8 = 8;
const VERTICAL: u8 = 16;
const SEEN: u8 = 32;
const WORKSPACE_BYTES_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScreenError {
    Coordinates,
    Resource,
    Allocation,
    WorkerSpawn,
    WorkerPanic,
}

impl ScreenError {
    pub(super) fn code(self) -> &'static str {
        match self {
            Self::Coordinates => "production-boundary-coordinate-overflow",
            Self::Resource => "production-boundary-workspace-budget-exhausted",
            Self::Allocation => "production-boundary-workspace-allocation-failed",
            Self::WorkerSpawn => "production-boundary-worker-spawn-failed",
            Self::WorkerPanic => "production-boundary-worker-panicked",
        }
    }
}

struct Workspace {
    width: usize,
    occupancy: Vec<u8>,
    cells: Vec<u8>,
    queue: Vec<usize>,
}

#[derive(Default)]
struct Work {
    visited: u64,
    probes: u64,
    hops: u64,
}

struct Outcome {
    blocked: Vec<String>,
    work: Work,
}

impl Workspace {
    fn new(max_x: i64, max_y: i64) -> Result<Self, ScreenError> {
        // If either upper bound excludes the seed, the reference can visit only the seed.
        let (width, height) = if max_x < -1 || max_y < -1 {
            (1, 1)
        } else {
            let dimension = |max: i64| {
                usize::try_from(i128::from(max) + 2).map_err(|_| ScreenError::Coordinates)
            };
            (dimension(max_x)?, dimension(max_y)?)
        };
        let count = width.checked_mul(height).ok_or(ScreenError::Resource)?;
        let bytes = count
            .checked_mul(2 + size_of::<usize>())
            .ok_or(ScreenError::Resource)?;
        if bytes > WORKSPACE_BYTES_LIMIT {
            return Err(ScreenError::Resource);
        }
        let mut occupancy = Vec::new();
        let mut cells = Vec::new();
        let mut queue = Vec::new();
        occupancy
            .try_reserve_exact(count)
            .map_err(|_| ScreenError::Allocation)?;
        cells
            .try_reserve_exact(count)
            .map_err(|_| ScreenError::Allocation)?;
        queue
            .try_reserve_exact(count)
            .map_err(|_| ScreenError::Allocation)?;
        occupancy.resize(count, 0);
        cells.resize(count, 0);
        Ok(Self {
            width,
            occupancy,
            cells,
            queue,
        })
    }

    fn index(&self, x: i64, y: i64) -> Option<usize> {
        let x = usize::try_from(i128::from(x) + 1).ok()?;
        let y = usize::try_from(i128::from(y) + 1).ok()?;
        (x < self.width && y < self.cells.len() / self.width).then(|| y * self.width + x)
    }

    fn mark(&mut self, x: i64, y: i64, flag: u8) {
        if let Some(index) = self.index(x, y) {
            self.occupancy[index] |= flag;
        }
    }

    fn reset(&mut self) {
        self.cells.copy_from_slice(&self.occupancy);
        self.queue.clear();
    }

    fn next(&self, index: usize, direction: usize) -> Option<usize> {
        match direction {
            0 => (index % self.width != 0).then(|| index - 1),
            1 => (index % self.width + 1 < self.width).then(|| index + 1),
            2 => index.checked_sub(self.width),
            _ => (index < self.cells.len() - self.width).then(|| index + self.width),
        }
    }

    fn flood(&mut self, blocked: u8) -> Work {
        let mut work = Work::default();
        self.cells[0] |= SEEN;
        self.queue.push(0);
        let mut head = 0;
        while head < self.queue.len() {
            if head.is_multiple_of(256) && crate::work_control::stopped() {
                break;
            }
            let current = self.queue[head];
            head += 1;
            work.visited += 1;
            for direction in 0..4 {
                work.probes += 1;
                let axis = if direction < 2 { HORIZONTAL } else { VERTICAL };
                let mut next = self.next(current, direction);
                while let Some(index) = next {
                    if self.cells[index] & axis == 0 {
                        break;
                    }
                    work.hops += 1;
                    next = self.next(index, direction);
                }
                if let Some(index) = next {
                    if self.cells[index] & (blocked | SEEN) == 0 {
                        self.cells[index] |= SEEN;
                        self.queue.push(index);
                    }
                }
            }
        }
        work
    }

    fn seen(&self, x: i64, y: i64) -> bool {
        self.index(x, y)
            .is_some_and(|index| self.cells[index] & SEEN != 0)
    }

    fn bytes(&self) -> usize {
        self.occupancy.capacity()
            + self.cells.capacity()
            + self.queue.capacity() * size_of::<usize>()
    }

    fn fork(&self) -> Result<Self, ScreenError> {
        let mut copy = Self::new(
            self.width as i64 - 2,
            (self.cells.len() / self.width) as i64 - 2,
        )?;
        copy.occupancy.copy_from_slice(&self.occupancy);
        Ok(copy)
    }
}

fn layer(transport: TransportKind) -> u8 {
    match transport {
        TransportKind::Belt => BELT,
        TransportKind::Pipe => PIPE,
    }
}

fn extent(node: &ConstructiveNode, horizontal: bool) -> Result<i64, ScreenError> {
    let placement = node.placements.iter().map(|p| {
        if horizontal {
            i128::from(p.x) + i128::from(p.width)
        } else {
            i128::from(p.y) + i128::from(p.height)
        }
    });
    let network = node
        .transport_networks
        .iter()
        .flat_map(|n| n.cells.iter())
        .map(|p| i128::from(if horizontal { p.x } else { p.y }) + 1);
    i64::try_from(placement.chain(network).max().unwrap_or(0) + 3)
        .map_err(|_| ScreenError::Coordinates)
}

pub(super) fn blocked_boundaries(
    node: &ConstructiveNode,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    requested_workers: usize,
) -> Result<Vec<String>, ScreenError> {
    let span = profile_span(ProfileOp::BoundaryEscape);
    let mut workspace = build_workspace(node)?;
    span.add_work("envelope-cells", workspace.cells.len() as u64);
    let commodities = node
        .boundary_requirements
        .iter()
        .map(|b| (b.transport, b.item.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let workers = parallel::worker_count(
        requested_workers,
        commodities.len(),
        workspace.bytes(),
        std::thread::available_parallelism().map_or(1, usize::from),
    );
    let bytes = workspace.bytes() * if workers > 1 { workers + 1 } else { 1 };
    span.add_work("workspace-bytes", bytes as u64);
    span.max_work("peak-workspace-bytes", bytes as u64);
    span.max_work("peak-workers", workers as u64);
    span.add_work("worker-slots", workers as u64);
    let outcomes = if workers > 1 {
        span.add_work("parallel-screens", 1);
        let _dispatch = profile_span(ProfileOp::BoundaryDispatch);
        parallel::screen(
            &workspace,
            &commodities,
            node,
            transports,
            components,
            workers,
        )?
    } else {
        commodities
            .iter()
            .map(|(transport, item)| {
                screen_commodity(
                    &mut workspace,
                    node,
                    transports,
                    components,
                    *transport,
                    item,
                )
            })
            .collect()
    };
    let mut blocked = Vec::new();
    for outcome in outcomes {
        span.add_work("commodity-screens", 1);
        span.add_work("cells-visited", outcome.work.visited);
        span.add_work("neighbor-probes", outcome.work.probes);
        span.add_work("crossing-hops", outcome.work.hops);
        blocked.extend(outcome.blocked);
    }
    blocked.sort();
    blocked.dedup();
    Ok(blocked)
}

fn screen_commodity(
    workspace: &mut Workspace,
    node: &ConstructiveNode,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    transport: TransportKind,
    item: &str,
) -> Outcome {
    workspace.reset();
    let cap = transports.capacity(transport);
    let rate = Rate::from_quantity_per_duration_ms(cap.quantity, cap.duration_ms).unwrap();
    let crossings = network::crossings::options(node, transport, rate, components);
    for ((x, y), (horizontal, _)) in crossings {
        if let Some(index) = workspace.index(x, y) {
            workspace.cells[index] |= if horizontal { HORIZONTAL } else { VERTICAL };
        }
    }
    let work = {
        let _span = profile_span(ProfileOp::BoundaryFlood);
        workspace.flood(FACILITY | layer(transport))
    };
    let mut blocked = Vec::new();
    for boundary in node
        .boundary_requirements
        .iter()
        .filter(|b| b.transport == transport && b.item == item)
    {
        if !boundary.attachments.iter().any(|attachment| {
            let p = match attachment {
                ConstructiveBoundaryAttachment::NewPort { port } => &port.connection,
                ConstructiveBoundaryAttachment::ExistingNetwork { branch_cell, .. } => branch_cell,
            };
            workspace.seen(p.x, p.y)
        }) {
            blocked.push(boundary.requirement.clone());
        }
    }
    Outcome { blocked, work }
}

fn build_workspace(node: &ConstructiveNode) -> Result<Workspace, ScreenError> {
    let _span = profile_span(ProfileOp::BoundaryWorkspace);
    let mut workspace = Workspace::new(extent(node, true)?, extent(node, false)?)?;
    for placement in &node.placements {
        let end_x = placement
            .x
            .checked_add(placement.width)
            .ok_or(ScreenError::Coordinates)?;
        let end_y = placement
            .y
            .checked_add(placement.height)
            .ok_or(ScreenError::Coordinates)?;
        // Only in-envelope occupancy can be reached. Iterate the intersection, not outside cells.
        for y in placement.y.max(-1)
            ..end_y.min(workspace.cells.len() as i64 / workspace.width as i64 - 1)
        {
            for x in placement.x.max(-1)..end_x.min(workspace.width as i64 - 1) {
                workspace.mark(x, y, FACILITY);
            }
        }
    }
    for network in &node.transport_networks {
        for cell in &network.cells {
            workspace.mark(cell.x, cell.y, layer(network.transport));
        }
    }
    Ok(workspace)
}

#[cfg(test)]
pub(super) mod reference;
#[cfg(test)]
mod tests;
