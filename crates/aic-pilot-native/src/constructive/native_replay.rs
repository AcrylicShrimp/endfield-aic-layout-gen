//! Retained native replay admission. The fork copy is the first resumable phase.

use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fmt::Write as _;
use std::future::{Future, poll_fn};
use std::hash::Hash;
use std::mem::size_of;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use aic_layout::facilities::FacilityPortDirection;
use aic_layout::geometry::{Point, direction as point_direction, layer};
use aic_layout::logistics::LogisticsComponentKind;
use aic_layout::physical::{
    Bridge, BridgeAxis, BridgeChannel, Junction, Layout, Metrics, Pose, Route, TerminalPose,
};
use aic_layout::projection::{cardinal_index, project_port_position, project_port_with_id};
use aic_layout::rate::Rate;
use aic_layout::validation::{
    Catalogs, JunctionAdvance, JunctionOptionsCursor, JunctionReservation, SiteArmUse,
    ValidationAdvance, ValidationCursor, ValidationReservation, ValidationVisit, within_capacity,
};

use super::routing_candidates::{AllowanceStop, RequestAllowance, RequestFailure, WorkKind};
use super::{
    ActiveRoute, BridgeReservation, BridgeRouteCheck, Command, CompletedLayoutCursor, Diagnostic,
    Direction, Focus, MaterializationAdvance, MaterializationReservation, ReplayWork, RouteView,
    Session, SharedCellCheck, UsedPort, error,
};

#[derive(Clone, Copy)]
struct GrowthSpec {
    needed_cells: usize,
    old_vec_capacity: usize,
    old_set_capacity: usize,
    old_vec_bytes: usize,
    old_set_bytes: usize,
}

#[derive(Clone, Copy)]
struct VecGrowthSpec {
    needed: usize,
    old_capacity: usize,
    element_bytes: usize,
}

#[derive(Clone, Copy)]
struct MapGrowthSpec {
    needed: usize,
    old_capacity: usize,
    bucket_bytes: usize,
}

#[derive(Clone, Copy)]
enum AdmissionRequest {
    ChargePrefix { costs: [usize; 64], len: usize },
    ChargeRemainder(usize),
    Work(WorkKind),
    Checkpoint,
    Reserve(usize),
    PlanGrowth(GrowthSpec),
    PlanVecGrowth(VecGrowthSpec),
    PlanMapGrowth(MapGrowthSpec),
    Release(usize),
}

enum AdmissionGrant {
    Items(usize),
    Bytes(usize),
    Growth(JointGrowthPlan),
    VecGrowth(usize),
    MapGrowth(usize),
    Done,
}

#[derive(Default)]
struct AdmissionControl {
    request: Option<AdmissionRequest>,
    response: Option<Result<AdmissionGrant, RequestFailure>>,
    reserved_bytes: usize,
    work: ReplayWorkDelta,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ReplayWorkDelta {
    pub(super) copied_bytes: usize,
    pub(super) route_cells: usize,
    pub(super) users: usize,
    pub(super) catalog: usize,
    pub(super) validation: usize,
}

impl ReplayWorkDelta {
    fn charge(&mut self, kind: WorkKind, amount: usize) {
        match kind {
            WorkKind::CopiedBytes => self.copied_bytes += amount,
            WorkKind::ReplayRouteCells => self.route_cells += amount,
            WorkKind::ReplayUsers => self.users += amount,
            WorkKind::CatalogVisits => self.catalog += amount,
            WorkKind::ValidationRecords => self.validation += amount,
            _ => unreachable!("native replay charges only its five work categories"),
        }
    }
}

#[derive(Clone, Default)]
struct Admission(Rc<RefCell<AdmissionControl>>);

impl Admission {
    async fn request(&self, request: AdmissionRequest) -> Result<AdmissionGrant, RequestFailure> {
        poll_fn(|_| {
            let mut control = self.0.borrow_mut();
            if let Some(response) = control.response.take() {
                Poll::Ready(response)
            } else {
                control.request = Some(request);
                Poll::Pending
            }
        })
        .await
    }

    async fn charge(&self, bytes: usize) -> Result<(), RequestFailure> {
        // Fixed-size record bookkeeping can span a page just like vector and
        // string copying. Retain the paid prefix instead of repeating one
        // indivisible charge that can never fit a small page.
        let mut remaining = bytes;
        while remaining > 0 {
            let count = remaining.min(64);
            let costs = [1; 64];
            let granted = self.charge_prefix(&costs[..count]).await?;
            remaining -= granted;
        }
        Ok(())
    }

    async fn copy_prefix(&self, offered: usize) -> Result<usize, RequestFailure> {
        debug_assert!((1..=64).contains(&offered));
        let costs = [1; 64];
        self.charge_prefix(&costs[..offered]).await
    }

    async fn reserve(&self, bytes: usize) -> Result<(), RequestFailure> {
        if bytes > 0 {
            self.request(AdmissionRequest::Reserve(bytes)).await?;
        }
        Ok(())
    }

    async fn reserve_joint_growth(
        &self,
        spec: GrowthSpec,
    ) -> Result<JointGrowthPlan, RequestFailure> {
        match self.request(AdmissionRequest::PlanGrowth(spec)).await? {
            AdmissionGrant::Growth(plan) => Ok(plan),
            _ => unreachable!("joint growth admission returns its chosen plan"),
        }
    }

    async fn reserve_vec_growth(&self, spec: VecGrowthSpec) -> Result<usize, RequestFailure> {
        match self.request(AdmissionRequest::PlanVecGrowth(spec)).await? {
            AdmissionGrant::VecGrowth(capacity) => Ok(capacity),
            _ => unreachable!("vector growth admission returns its chosen capacity"),
        }
    }

    async fn reserve_map_growth(&self, spec: MapGrowthSpec) -> Result<usize, RequestFailure> {
        match self.request(AdmissionRequest::PlanMapGrowth(spec)).await? {
            AdmissionGrant::MapGrowth(capacity) => Ok(capacity),
            _ => unreachable!("map growth admission returns its chosen capacity"),
        }
    }

    async fn work(&self, kind: WorkKind) -> Result<(), RequestFailure> {
        self.request(AdmissionRequest::Work(kind)).await?;
        Ok(())
    }

    async fn checkpoint(&self) -> Result<(), RequestFailure> {
        self.request(AdmissionRequest::Checkpoint).await?;
        Ok(())
    }

    async fn release(&self, bytes: usize) {
        if bytes > 0 {
            self.request(AdmissionRequest::Release(bytes))
                .await
                .expect("releasing an owned reservation cannot fail");
        }
    }

    /// Admit the longest prefix that fits the current page and hard allowance.
    /// The caller copies exactly that prefix before asking for more credit.
    async fn charge_prefix(&self, costs: &[usize]) -> Result<usize, RequestFailure> {
        debug_assert!(!costs.is_empty() && costs.len() <= 64);
        let mut array = [0; 64];
        array[..costs.len()].copy_from_slice(costs);
        let first = self
            .request(AdmissionRequest::ChargePrefix {
                costs: array,
                len: costs.len(),
            })
            .await?;
        match first {
            AdmissionGrant::Items(count) => Ok(count),
            AdmissionGrant::Bytes(paid) => {
                let mut remaining = costs[0] - paid;
                while remaining > 0 {
                    let AdmissionGrant::Bytes(next) = self
                        .request(AdmissionRequest::ChargeRemainder(remaining))
                        .await?
                    else {
                        unreachable!("remainder admission returns charged bytes");
                    };
                    remaining -= next;
                }
                Ok(1)
            }
            AdmissionGrant::Done
            | AdmissionGrant::Growth(_)
            | AdmissionGrant::VecGrowth(_)
            | AdmissionGrant::MapGrowth(_) => {
                unreachable!("copy admission cannot finish non-copy work")
            }
        }
    }
}

fn is_retryable(
    stop: &AllowanceStop,
    allowance: &RequestAllowance,
    request: AdmissionRequest,
    pinned_external_bytes: usize,
) -> bool {
    stop.reason == "page-quantum"
        || stop.reason == "aggregate-memory-limit"
            && match request {
                AdmissionRequest::Reserve(bytes) => allowance
                    .aggregate_reservation_can_fit_if_others_release(bytes, pinned_external_bytes),
                AdmissionRequest::PlanGrowth(_)
                | AdmissionRequest::PlanVecGrowth(_)
                | AdmissionRequest::PlanMapGrowth(_) => true,
                _ => false,
            }
}

fn planned_map_capacity(spec: MapGrowthSpec, available_bytes: usize) -> Option<usize> {
    let bytes = |capacity| hash_bucket_count(capacity).saturating_mul(spec.bucket_bytes);
    if bytes(spec.needed) > available_bytes {
        return None;
    }
    let mut lower = spec.needed;
    let mut upper = spec.old_capacity.saturating_mul(2).max(spec.needed);
    while lower < upper {
        let middle = lower + (upper - lower) / 2 + 1;
        if bytes(middle) <= available_bytes {
            lower = middle;
        } else {
            upper = middle - 1;
        }
    }
    Some(lower)
}

enum AdmittedAdvance<T> {
    Pending(AllowanceStop),
    Complete(T),
}

fn poll_admitted<'a, T>(
    future: &mut Option<Pin<Box<dyn Future<Output = T> + 'a>>>,
    admission: &Admission,
    allowance: &mut RequestAllowance,
    pinned_external_bytes: usize,
) -> AdmittedAdvance<T> {
    let mut context = Context::from_waker(Waker::noop());
    loop {
        let pending = admission.0.borrow_mut().request.take();
        if let Some(request) = pending {
            let result = match request {
                AdmissionRequest::ChargePrefix { costs, len } => {
                    let remaining = allowance.page_remaining(WorkKind::CopiedBytes);
                    let mut granted = 0;
                    let mut bytes = 0usize;
                    for cost in costs.into_iter().take(len) {
                        if cost > remaining.saturating_sub(bytes) {
                            break;
                        }
                        bytes += cost;
                        granted += 1;
                    }
                    if granted == 0 {
                        let chunk = costs[0].min(remaining);
                        if chunk == 0 {
                            allowance
                                .charge(WorkKind::CopiedBytes, 1)
                                .map(|_| AdmissionGrant::Bytes(1))
                        } else {
                            allowance
                                .charge(WorkKind::CopiedBytes, chunk)
                                .map(|_| AdmissionGrant::Bytes(chunk))
                        }
                    } else {
                        allowance
                            .charge(WorkKind::CopiedBytes, bytes)
                            .map(|_| AdmissionGrant::Items(granted))
                    }
                }
                AdmissionRequest::ChargeRemainder(remaining) => {
                    let chunk = remaining.min(allowance.page_remaining(WorkKind::CopiedBytes));
                    allowance
                        .charge(WorkKind::CopiedBytes, chunk.max(1))
                        .map(|_| AdmissionGrant::Bytes(chunk.max(1)))
                }
                AdmissionRequest::Work(kind) => {
                    allowance.charge(kind, 1).map(|_| AdmissionGrant::Done)
                }
                AdmissionRequest::Checkpoint => {
                    allowance.check_deadline().map(|_| AdmissionGrant::Done)
                }
                AdmissionRequest::Reserve(bytes) => {
                    allowance.reserve_peak(bytes).map(|_| AdmissionGrant::Done)
                }
                AdmissionRequest::PlanGrowth(spec) => {
                    let stable_peak_cap = allowance.stable_peak_cap(pinned_external_bytes);
                    let plan = plan_joint_extend_growth(JointGrowthInput {
                        needed_cells: spec.needed_cells,
                        old_vec_capacity: spec.old_vec_capacity,
                        old_set_capacity: spec.old_set_capacity,
                        old_vec_bytes: spec.old_vec_bytes,
                        old_set_bytes: spec.old_set_bytes,
                        current_reserved_bytes: allowance.reserved_bytes(),
                        stable_peak_cap,
                    });
                    match plan {
                        Some(plan) => allowance
                            .reserve_peak(plan.escrow_bytes)
                            .map(|_| AdmissionGrant::Growth(plan)),
                        None => {
                            let stop = AllowanceStop {
                                reason: "peak-memory-limit",
                                category: None,
                                limit: stable_peak_cap,
                                used: allowance.reserved_bytes(),
                                deadline_overshoot_ms: 0,
                                attempted_bytes: None,
                                aggregate_peak_bytes: None,
                            };
                            allowance.confirm_hard_stop(&stop);
                            Err(stop)
                        }
                    }
                }
                AdmissionRequest::PlanVecGrowth(spec) => {
                    let stable_peak_cap = allowance.stable_peak_cap(pinned_external_bytes);
                    let headroom = stable_peak_cap.saturating_sub(allowance.reserved_bytes());
                    let max_capacity = headroom / spec.element_bytes;
                    if max_capacity < spec.needed {
                        let stop = AllowanceStop {
                            reason: "peak-memory-limit",
                            category: None,
                            limit: stable_peak_cap,
                            used: allowance.reserved_bytes(),
                            deadline_overshoot_ms: 0,
                            attempted_bytes: None,
                            aggregate_peak_bytes: None,
                        };
                        allowance.confirm_hard_stop(&stop);
                        Err(stop)
                    } else {
                        let capacity = spec
                            .old_capacity
                            .saturating_mul(2)
                            .max(spec.needed)
                            .min(max_capacity);
                        allowance
                            .reserve_peak(capacity * spec.element_bytes)
                            .map(|_| AdmissionGrant::VecGrowth(capacity))
                    }
                }
                AdmissionRequest::PlanMapGrowth(spec) => {
                    let stable_peak_cap = allowance.stable_peak_cap(pinned_external_bytes);
                    let available = stable_peak_cap.saturating_sub(allowance.reserved_bytes());
                    match planned_map_capacity(spec, available) {
                        Some(capacity) => allowance
                            .reserve_peak(
                                hash_bucket_count(capacity).saturating_mul(spec.bucket_bytes),
                            )
                            .map(|_| AdmissionGrant::MapGrowth(capacity)),
                        None => {
                            let stop = AllowanceStop {
                                reason: "peak-memory-limit",
                                category: None,
                                limit: stable_peak_cap,
                                used: allowance.reserved_bytes(),
                                deadline_overshoot_ms: 0,
                                attempted_bytes: None,
                                aggregate_peak_bytes: None,
                            };
                            allowance.confirm_hard_stop(&stop);
                            Err(stop)
                        }
                    }
                }
                AdmissionRequest::Release(bytes) => {
                    allowance.release_peak(bytes);
                    admission.0.borrow_mut().reserved_bytes -= bytes;
                    Ok(AdmissionGrant::Done)
                }
            };
            match result {
                Ok(granted) => {
                    let charged = match (request, &granted) {
                        (
                            AdmissionRequest::ChargePrefix { costs, .. },
                            AdmissionGrant::Items(count),
                        ) => Some((WorkKind::CopiedBytes, costs[..*count].iter().sum())),
                        (
                            AdmissionRequest::ChargePrefix { .. }
                            | AdmissionRequest::ChargeRemainder(_),
                            AdmissionGrant::Bytes(bytes),
                        ) => Some((WorkKind::CopiedBytes, *bytes)),
                        (AdmissionRequest::Work(kind), _) => Some((kind, 1)),
                        _ => None,
                    };
                    if let Some((kind, amount)) = charged {
                        admission.0.borrow_mut().work.charge(kind, amount);
                    }
                    let reserved = match (request, &granted) {
                        (AdmissionRequest::Reserve(bytes), _) => bytes,
                        (AdmissionRequest::PlanGrowth(_), AdmissionGrant::Growth(plan)) => {
                            plan.escrow_bytes
                        }
                        (
                            AdmissionRequest::PlanVecGrowth(spec),
                            AdmissionGrant::VecGrowth(capacity),
                        ) => capacity * spec.element_bytes,
                        (
                            AdmissionRequest::PlanMapGrowth(spec),
                            AdmissionGrant::MapGrowth(capacity),
                        ) => hash_bucket_count(*capacity).saturating_mul(spec.bucket_bytes),
                        _ => 0,
                    };
                    admission.0.borrow_mut().reserved_bytes += reserved;
                    admission.0.borrow_mut().response = Some(Ok(granted));
                }
                Err(stop) if is_retryable(&stop, allowance, request, pinned_external_bytes) => {
                    admission.0.borrow_mut().request = Some(request);
                    return AdmittedAdvance::Pending(stop);
                }
                Err(stop) => {
                    admission.0.borrow_mut().response = Some(Err(RequestFailure::Stopped(stop)));
                }
            }
            continue;
        }
        match future
            .as_mut()
            .expect("admitted future is active")
            .as_mut()
            .poll(&mut context)
        {
            Poll::Ready(result) => {
                *future = None;
                return AdmittedAdvance::Complete(result);
            }
            Poll::Pending => {
                debug_assert!(admission.0.borrow().request.is_some());
            }
        }
    }
}

/// The source remains owned by the pinned future until the fork either
/// completes or is explicitly cancelled with its whole candidate owner.
pub(super) enum SourceOwner<'a> {
    Borrowed(&'a Session),
    Shared(Arc<Session>),
    Owned(Session),
}

impl SourceOwner<'_> {
    fn session(&self) -> &Session {
        match self {
            Self::Borrowed(source) => source,
            Self::Shared(source) => source,
            Self::Owned(source) => source,
        }
    }
}

async fn copy_owned_source(
    source: SourceOwner<'_>,
    admission: Admission,
) -> (SourceOwner<'_>, Result<Session, RequestFailure>) {
    let result = copy_session(source.session(), admission).await;
    (source, result)
}

/// The partially copied fork and its original source survive a page stop.
pub(super) struct SessionForkCursor<'a> {
    future: Option<
        Pin<Box<dyn Future<Output = (SourceOwner<'a>, Result<Session, RequestFailure>)> + 'a>>,
    >,
    admission: Admission,
}

pub(super) enum ForkAdvance<'a> {
    Pending(AllowanceStop),
    Complete(SourceOwner<'a>, Result<(Session, usize), RequestFailure>),
}

impl<'a> SessionForkCursor<'a> {
    pub(super) fn new(source: &'a Session) -> Self {
        Self::new_owner(SourceOwner::Borrowed(source))
    }

    pub(super) fn new_shared(source: Arc<Session>) -> Self {
        Self::new_owner(SourceOwner::Shared(source))
    }

    pub(super) fn new_owned(source: Session) -> Self {
        Self::new_owner(SourceOwner::Owned(source))
    }

    fn new_owner(source: SourceOwner<'a>) -> Self {
        let admission = Admission::default();
        let future = Box::pin(copy_owned_source(source, admission.clone()));
        Self {
            future: Some(future),
            admission,
        }
    }

    pub(super) fn reserved_bytes(&self) -> usize {
        self.admission.0.borrow().reserved_bytes
    }

    pub(super) fn cancel(mut self, allowance: &mut RequestAllowance) {
        self.future = None;
        let bytes = std::mem::take(&mut self.admission.0.borrow_mut().reserved_bytes);
        if bytes > 0 {
            allowance.release_peak(bytes);
        }
    }

    pub(super) fn advance(
        &mut self,
        allowance: &mut RequestAllowance,
        pinned_external_bytes: usize,
    ) -> ForkAdvance<'a> {
        match poll_admitted(
            &mut self.future,
            &self.admission,
            allowance,
            pinned_external_bytes,
        ) {
            AdmittedAdvance::Pending(stop) => ForkAdvance::Pending(stop),
            AdmittedAdvance::Complete((source, result)) => {
                let bytes = std::mem::take(&mut self.admission.0.borrow_mut().reserved_bytes);
                match result {
                    Ok(fork) => ForkAdvance::Complete(source, Ok((fork, bytes))),
                    Err(failure) => {
                        allowance.release_peak(bytes);
                        ForkAdvance::Complete(source, Err(failure))
                    }
                }
            }
        }
    }
}

/// The same native cell-capacity rule used by ordinary commands, with one
/// retained allowance visit before each committed user is examined.
async fn shared_cell_available(
    session: &Session,
    point: Point,
    connection: usize,
    admission: &Admission,
) -> Result<bool, RequestFailure> {
    let mut check = SharedCellCheck::new(session, point, connection);
    while check.has_next_user() {
        admission.work(WorkKind::ReplayUsers).await?;
        check.advance_user();
    }
    Ok(check.finish())
}

async fn route_endpoint_admitted(
    session: &Session,
    endpoint: &str,
    selected_port: Option<&str>,
    expected: FacilityPortDirection,
    obligation: &aic_layout::physical::Connection,
    admission: &Admission,
) -> Result<(Point, Option<String>), RequestFailure> {
    let Some(&block) = session.blocks.get(endpoint) else {
        return session
            .route_endpoint(endpoint, selected_port, expected, obligation)
            .map(|(point, _)| (point, None))
            .map_err(RequestFailure::Invalid);
    };
    let id = selected_port.ok_or_else(|| {
        RequestFailure::Invalid(error(
            "missing-port",
            &obligation.id,
            "facility route endpoint requires a selected port",
        ))
    })?;
    let pose = session.poses[block].as_ref().ok_or_else(|| {
        RequestFailure::Invalid(error(
            "unplaced-endpoint",
            &session.problem.blocks[block].id,
            "route endpoint facility is not placed",
        ))
    })?;
    let definition = session
        .catalogs
        .facilities
        .facility(&session.problem.blocks[block].facility)
        .expect("problem admission checked facilities");
    let mut selected = None;
    for port in &definition.ports {
        admission.work(WorkKind::CatalogVisits).await?;
        if port.id == id {
            selected = Some(port);
            break;
        }
    }
    let selected = selected.ok_or_else(|| {
        RequestFailure::Invalid(error(
            "unknown-port",
            &obligation.id,
            "selected port does not exist",
        ))
    })?;
    let owned_id = string(id, admission).await?;
    let projected =
        project_port_with_id(definition, selected, pose.rotation, pose.position, owned_id);
    let key = (block, projected.id);
    let validation = session.validate_selected_port(
        &key,
        expected,
        obligation.transport,
        &obligation.commodity,
        obligation.rate,
        &obligation.id,
        projected.direction,
        projected.transport,
    );
    match validation {
        Ok(()) => Ok((projected.connection, Some(key.1))),
        Err(diagnostic) => {
            let bytes = key.1.len();
            drop(key);
            admission.release(bytes).await;
            Err(RequestFailure::Invalid(diagnostic))
        }
    }
}

struct PreparedBeginRoute {
    connection: usize,
    source: Point,
    target: Point,
    source_port: Option<String>,
    target_port: Option<String>,
}

async fn prepare_begin_route(
    session: &Session,
    connection: &str,
    source_port: Option<&str>,
    target_port: Option<&str>,
    admission: &Admission,
) -> Result<PreparedBeginRoute, RequestFailure> {
    if session.active.is_some() {
        return Err(RequestFailure::Invalid(error(
            "active-route",
            connection,
            "only one route may be active in the first constructive domain",
        )));
    }
    let Some(&index) = session.connections.get(connection) else {
        return Err(RequestFailure::Invalid(error(
            "unknown-connection",
            connection,
            "begin-route references an unknown logical connection",
        )));
    };
    if session.routes[index].is_some() {
        return Err(RequestFailure::Invalid(error(
            "completed-connection",
            connection,
            "logical connection already has a completed route",
        )));
    }
    let obligation = &session.problem.connections[index];
    let (source, source_port) = route_endpoint_admitted(
        session,
        &obligation.source,
        source_port,
        FacilityPortDirection::Output,
        obligation,
        admission,
    )
    .await?;
    let target_result = route_endpoint_admitted(
        session,
        &obligation.target,
        target_port,
        FacilityPortDirection::Input,
        obligation,
        admission,
    )
    .await;
    let (target, target_port) = match target_result {
        Ok(target) => target,
        Err(failure) => {
            let bytes = source_port.as_ref().map_or(0, String::len);
            drop(source_port);
            admission.release(bytes).await;
            return Err(failure);
        }
    };
    let checked: Result<(), RequestFailure> = async {
        if session.bodies.contains(&source) || session.bodies.contains(&target) {
            return Err(RequestFailure::Invalid(error(
                "blocked-port",
                connection,
                "a selected port connection cell is covered by a facility body",
            )));
        }
        if !shared_cell_available(session, source, index, admission).await?
            || !shared_cell_available(session, target, index, admission).await?
        {
            return Err(RequestFailure::Invalid(error(
                "occupied-port",
                connection,
                "a selected port connection cell is occupied on the same layer",
            )));
        }
        let set_bytes = if session.active_cells.capacity() == 0 {
            hash_bucket_count(1) * (size_of::<Point>() + 1)
        } else {
            0
        };
        admission.reserve(size_of::<Point>() + set_bytes).await
    }
    .await;
    if let Err(failure) = checked {
        let bytes = source_port.as_ref().map_or(0, String::len)
            + target_port.as_ref().map_or(0, String::len);
        drop(source_port);
        drop(target_port);
        admission.release(bytes).await;
        return Err(failure);
    }
    Ok(PreparedBeginRoute {
        connection: index,
        source,
        target,
        source_port,
        target_port,
    })
}

fn commit_begin_route(
    session: &mut Session,
    prepared: PreparedBeginRoute,
    ledger: &mut ActiveStorageLedger,
) {
    session.active = Some(ActiveRoute {
        connection: prepared.connection,
        source_port: prepared.source_port,
        target_port: prepared.target_port,
        target: prepared.target,
        cells: vec![prepared.source],
        bridge_reservations: Vec::new(),
    });
    session.active_cells.clear();
    session.active_cells.insert(prepared.source);
    session.focus = Some(Focus::Route {
        connection: prepared.connection,
        endpoint: prepared.source,
        target: prepared.target,
        active: true,
    });
    *ledger = ActiveStorageLedger::for_fork(session);
}

async fn find_route_cell(
    route: &Route,
    position: Point,
    admission: &Admission,
) -> Result<Option<usize>, RequestFailure> {
    for (index, &cell) in route.cells.iter().enumerate() {
        admission.work(WorkKind::ReplayRouteCells).await?;
        if cell == position {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

async fn site_arm_port_position_admitted(
    session: &Session,
    block: usize,
    port_id: &str,
    connection: &str,
    source: bool,
    admission: &Admission,
) -> Result<Point, RequestFailure> {
    let pose = session.poses[block].as_ref().ok_or_else(|| {
        RequestFailure::Invalid(error(
            "unplaced-endpoint",
            &session.problem.blocks[block].id,
            "route endpoint facility is not placed",
        ))
    })?;
    let definition = session
        .catalogs
        .facilities
        .facility(&session.problem.blocks[block].facility)
        .expect("problem admission checked facilities");
    for port in &definition.ports {
        admission.work(WorkKind::CatalogVisits).await?;
        if port.id == port_id {
            return Ok(project_port_position(
                definition,
                port,
                pose.rotation,
                pose.position,
            ));
        }
    }
    Err(RequestFailure::Invalid(error(
        "topology",
        connection,
        if source {
            "source route port is unknown"
        } else {
            "target route port is unknown"
        },
    )))
}

async fn site_arm_use_admitted<'a>(
    session: &'a Session,
    connection_index: usize,
    route: RouteView<'_>,
    path_index: usize,
    admission: &Admission,
) -> Result<SiteArmUse<'a>, RequestFailure> {
    let connection = &session.problem.connections[connection_index];
    let source_position = if path_index == 0 {
        if let Some(&block) = session.blocks.get(&connection.source) {
            let port_id = route.source_port.ok_or_else(|| {
                RequestFailure::Invalid(error(
                    "topology",
                    &connection.id,
                    "source route port is missing",
                ))
            })?;
            Some(
                site_arm_port_position_admitted(
                    session,
                    block,
                    port_id,
                    &connection.id,
                    true,
                    admission,
                )
                .await?,
            )
        } else {
            None
        }
    } else {
        None
    };
    let target_position = if path_index + 1 == route.cells.len() {
        if let Some(&block) = session.blocks.get(&connection.target) {
            let port_id = route.target_port.ok_or_else(|| {
                RequestFailure::Invalid(error(
                    "topology",
                    &connection.id,
                    "target route port is missing",
                ))
            })?;
            Some(
                site_arm_port_position_admitted(
                    session,
                    block,
                    port_id,
                    &connection.id,
                    false,
                    admission,
                )
                .await?,
            )
        } else {
            None
        }
    } else {
        None
    };
    session
        .site_arm_use_with_positions(
            connection_index,
            route,
            path_index,
            source_position,
            target_position,
        )
        .map_err(RequestFailure::Invalid)
}

async fn check_bridge_route_admitted(
    pending: RouteView<'_>,
    reserved: &BridgeReservation,
    connection: &str,
    admission: &Admission,
) -> Result<(), RequestFailure> {
    admission.checkpoint().await?;
    let mut check = BridgeRouteCheck::new(pending.cells, reserved);
    while check.has_next_cell() {
        admission.work(WorkKind::ReplayRouteCells).await?;
        if check.next_cell % 256 == 0 {
            admission.checkpoint().await?;
        }
        check.advance_cell();
    }
    check.finish(connection).map_err(RequestFailure::Invalid)
}

fn decimal_length(value: i64) -> usize {
    let magnitude = value.unsigned_abs();
    usize::from(value < 0)
        + if magnitude == 0 {
            1
        } else {
            magnitude.ilog10() as usize + 1
        }
}

struct SiteIdBuffer {
    bytes: [u8; 64],
    len: usize,
}

impl std::fmt::Write for SiteIdBuffer {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let end = self.len + value.len();
        let destination = self.bytes.get_mut(self.len..end).ok_or(std::fmt::Error)?;
        destination.copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

async fn formatted_site_id(
    kind: &str,
    transport_id: &str,
    position: Point,
    admission: &Admission,
) -> Result<String, RequestFailure> {
    let bytes = kind.len()
        + 1
        + transport_id.len()
        + 1
        + decimal_length(position.x)
        + 1
        + decimal_length(position.y);
    admission.reserve(bytes).await?;
    let mut buffer = SiteIdBuffer {
        bytes: [0; 64],
        len: 0,
    };
    write!(
        buffer,
        "{kind}-{transport_id}-{}-{}",
        position.x, position.y
    )
    .expect("fixed native site ID fits stack buffer");
    debug_assert_eq!(buffer.len, bytes);
    let source = std::str::from_utf8(&buffer.bytes[..buffer.len])
        .expect("native site ID contains only ASCII pieces");
    let mut id = String::with_capacity(bytes);
    while id.len() < bytes {
        let granted = admission.copy_prefix((bytes - id.len()).min(64)).await?;
        id.push_str(&source[id.len()..id.len() + granted]);
    }
    Ok(id)
}

/// Build every bridge replacement from a borrowed active path before Connect
/// mutates the private fork. The shared route check owns the cell index across
/// page stops; string and owner copies use retained admission as well.
async fn prepare_bridge_updates(
    session: &Session,
    pending_index: usize,
    pending: RouteView<'_>,
    admission: &Admission,
) -> Result<Vec<Bridge>, RequestFailure> {
    let active = session
        .active
        .as_ref()
        .expect("bridge updates require an active route");
    let obligation = &session.problem.connections[pending_index];
    let transport = obligation.transport;
    let transport_id = match transport {
        aic_layout::logistics::TransportKind::Belt => "belt",
        aic_layout::logistics::TransportKind::Pipe => "pipe",
    };
    let mut bridges = vector::<Bridge>(active.bridge_reservations.len(), admission).await?;
    for reserved in &active.bridge_reservations {
        check_bridge_route_admitted(pending, reserved, &obligation.id, admission).await?;
        if let Some(installed) = session.bridges.get(&(transport, reserved.position)) {
            let axis = reserved.direction % 2;
            for _ in &installed.channels[axis].connections {
                admission.work(WorkKind::ReplayUsers).await?;
            }
            admission.charge(size_of::<Bridge>()).await?;
            let id = string(&installed.id, admission).await?;
            let component = string(&installed.component, admission).await?;
            let horizontal = bridge_channel_with_extra(
                &installed.channels[0],
                (axis == 0).then_some(obligation.id.as_str()),
                admission,
            )
            .await?;
            let vertical = bridge_channel_with_extra(
                &installed.channels[1],
                (axis == 1).then_some(obligation.id.as_str()),
                admission,
            )
            .await?;
            bridges.push(Bridge {
                id,
                component,
                position: installed.position,
                rotation: installed.rotation,
                channels: [horizontal, vertical],
            });
            continue;
        }
        let users = session.occupied[layer(transport)]
            .get(&reserved.position)
            .expect("reserved bridge has committed carrier users");
        let heap_bytes = users.len().saturating_mul(size_of::<Reverse<&str>>());
        admission.reserve(heap_bytes).await?;
        let mut owner_heap = BinaryHeap::with_capacity(users.len());
        for &ci in users {
            admission.work(WorkKind::ReplayUsers).await?;
            owner_heap.push(Reverse(session.problem.connections[ci].id.as_str()));
        }
        let mut owners = vector::<String>(users.len(), admission).await?;
        while let Some(Reverse(id)) = owner_heap.peek().copied() {
            admission.charge(size_of::<String>()).await?;
            let copied = string(id, admission).await?;
            owner_heap.pop();
            owners.push(copied);
        }
        drop(owner_heap);
        admission.release(heap_bytes).await;
        let carrier_commodity =
            string(&session.problem.connections[users[0]].commodity, admission).await?;
        let new_commodity = string(&obligation.commodity, admission).await?;
        let mut new_owners = vector::<String>(1, admission).await?;
        admission.charge(size_of::<String>()).await?;
        new_owners.push(string(&obligation.id, admission).await?);
        let carrier = BridgeChannel {
            axis: reserved.crossed_axis,
            commodity: carrier_commodity,
            connections: owners,
        };
        let entering_axis = if reserved.crossed_axis == BridgeAxis::Horizontal {
            BridgeAxis::Vertical
        } else {
            BridgeAxis::Horizontal
        };
        let entering = BridgeChannel {
            axis: entering_axis,
            commodity: new_commodity,
            connections: new_owners,
        };
        admission.charge(size_of::<Bridge>()).await?;
        let id = formatted_site_id("bridge", transport_id, reserved.position, admission).await?;
        let component = string(&reserved.component, admission).await?;
        bridges.push(Bridge {
            id,
            component,
            position: reserved.position,
            rotation: reserved.rotation,
            channels: if reserved.crossed_axis == BridgeAxis::Horizontal {
                [carrier, entering]
            } else {
                [entering, carrier]
            },
        });
    }
    Ok(bridges)
}

#[derive(Clone, Copy)]
enum JunctionNeed {
    User,
    Catalog,
    Copy(usize),
    Reserve(usize),
}

/// Drive the established junction rule one use, catalog visit, or result
/// allocation at a time. Site-arm construction already charges ReplayUsers;
/// processing those arms adds only a page/deadline checkpoint.
async fn junction_options_admitted(
    transport: aic_layout::logistics::TransportKind,
    point: Point,
    uses: &[SiteArmUse<'_>],
    catalogs: &Catalogs,
    admission: &Admission,
) -> Result<(Option<Vec<(String, i64)>>, usize), RequestFailure> {
    let mut cursor = JunctionOptionsCursor::new(transport, point);
    let mut grant = None;
    loop {
        let requested = Cell::new(None);
        let released = Cell::new(0usize);
        let mut user_credit = matches!(grant, Some(JunctionNeed::User));
        let mut catalog_credit = matches!(grant, Some(JunctionNeed::Catalog));
        let mut copy_credit = match grant {
            Some(JunctionNeed::Copy(bytes)) => Some(bytes),
            _ => None,
        };
        let mut reserve_credit = match grant {
            Some(JunctionNeed::Reserve(bytes)) => Some(bytes),
            _ => None,
        };
        let advance = cursor.advance(
            uses,
            catalogs,
            || {
                if std::mem::take(&mut user_credit) {
                    true
                } else {
                    requested.set(Some(JunctionNeed::User));
                    false
                }
            },
            || {
                if std::mem::take(&mut catalog_credit) {
                    true
                } else {
                    requested.set(Some(JunctionNeed::Catalog));
                    false
                }
            },
            |bytes| {
                if let Some(granted) = copy_credit.take() {
                    granted
                } else {
                    requested.set(Some(JunctionNeed::Copy(bytes)));
                    0
                }
            },
            |bytes| {
                if reserve_credit.take() == Some(bytes) {
                    JunctionReservation::Granted
                } else {
                    requested.set(Some(JunctionNeed::Reserve(bytes)));
                    JunctionReservation::Retry
                }
            },
            |bytes| released.set(released.get().saturating_add(bytes)),
        );
        admission.release(released.get()).await;
        match advance {
            JunctionAdvance::Pending | JunctionAdvance::Retry => {
                grant = requested.get();
                let result = match grant.expect("paused junction cursor requests one admission") {
                    JunctionNeed::User => admission.checkpoint().await,
                    JunctionNeed::Catalog => admission.work(WorkKind::CatalogVisits).await,
                    JunctionNeed::Copy(bytes) => {
                        admission.copy_prefix(bytes).await.map(|granted| {
                            grant = Some(JunctionNeed::Copy(granted));
                        })
                    }
                    JunctionNeed::Reserve(bytes) => admission.reserve(bytes).await,
                };
                if let Err(failure) = result {
                    admission.release(cursor.reserved_bytes()).await;
                    return Err(failure);
                }
            }
            JunctionAdvance::Denied => unreachable!("admission decides hard resource stops"),
            JunctionAdvance::Complete(Ok(options)) => {
                return Ok((options, cursor.reserved_bytes()));
            }
            JunctionAdvance::Complete(Err(failure)) => {
                admission.release(cursor.reserved_bytes()).await;
                return Err(RequestFailure::Invalid(error(
                    "topology",
                    failure.entity.to_string(),
                    failure.message,
                )));
            }
        }
    }
}

type JunctionUpdate = (
    (aic_layout::logistics::TransportKind, Point),
    Option<Junction>,
);

struct PreparedJunctionUpdates {
    updates: Vec<JunctionUpdate>,
    new_occupied_sites: usize,
}

/// Retain the ordinary junction rule's touched sites and old route indices
/// while paging every route, user and catalog visit. All native-invalid
/// topology decisions finish before the private Connect commit begins.
async fn prepare_junction_updates(
    session: &Session,
    pending_index: usize,
    pending: RouteView<'_>,
    admission: &Admission,
) -> Result<PreparedJunctionUpdates, RequestFailure> {
    let baseline_bytes = admission.0.borrow().reserved_bytes;
    let scratch_bytes = Cell::new(0usize);
    let result = async {
        let transport = session.problem.connections[pending_index].transport;
        let transport_id = match transport {
            aic_layout::logistics::TransportKind::Belt => "belt",
            aic_layout::logistics::TransportKind::Pipe => "pipe",
        };
        // BeginRoute and Extend maintain this set from the active path. The
        // ordinary path rebuilds it; retained replay can borrow its fork copy.
        let touched = &session.active_cells;
        let mut touched_users = 0usize;
        let mut new_occupied_sites = 0usize;
        for &point in pending.cells {
            admission.work(WorkKind::ReplayRouteCells).await?;
            if let Some(users) = session.occupied[layer(transport)].get(&point) {
                touched_users = touched_users.saturating_add(users.len());
            } else {
                new_occupied_sites += 1;
            }
        }
        let existing_capacity = touched_users.min(session.routes.len());
        let mut existing = set::<usize>(existing_capacity, admission).await?;
        scratch_bytes.set(scratch_bytes.get() + set_storage_bytes::<usize>(existing_capacity));
        for point in touched {
            admission.checkpoint().await?;
            if let Some(users) = session.occupied[layer(transport)].get(point) {
                for &ci in users {
                    admission.work(WorkKind::ReplayUsers).await?;
                    if !existing.contains(&ci) {
                        admission.charge(size_of::<usize>()).await?;
                        existing.insert(ci);
                    }
                }
            }
        }
        let mut old_indices = map::<(usize, Point), usize>(touched_users, admission).await?;
        scratch_bytes
            .set(scratch_bytes.get() + map_storage_bytes::<(usize, Point), usize>(touched_users));
        for &ci in &existing {
            admission.checkpoint().await?;
            let route = session.routes[ci]
                .as_ref()
                .expect("occupied route is committed");
            for (pi, &point) in route.cells.iter().enumerate() {
                admission.work(WorkKind::ReplayRouteCells).await?;
                if touched.contains(&point) {
                    admission
                        .charge(size_of::<((usize, Point), usize)>())
                        .await?;
                    old_indices.insert((ci, point), pi);
                }
            }
        }
        let active = session
            .active
            .as_ref()
            .expect("junction updates need active route");
        let mut bridge_positions =
            set::<Point>(active.bridge_reservations.len(), admission).await?;
        scratch_bytes.set(
            scratch_bytes.get() + set_storage_bytes::<Point>(active.bridge_reservations.len()),
        );
        for reserved in &active.bridge_reservations {
            admission.checkpoint().await?;
            admission.charge(size_of::<Point>()).await?;
            bridge_positions.insert(reserved.position);
        }
        let mut updates = Vec::<JunctionUpdate>::new();
        for (pi, &point) in pending.cells.iter().enumerate() {
            admission.work(WorkKind::ReplayRouteCells).await?;
            if bridge_positions.contains(&point) {
                continue;
            }
            let users = session.occupied[layer(transport)].get(&point);
            if users.is_none() && pi > 0 && pi + 1 < pending.cells.len() {
                continue;
            }
            let uses_capacity = users.map_or(1, |users| users.len() + 1);
            let uses_bytes = uses_capacity.saturating_mul(size_of::<SiteArmUse<'_>>());
            let mut uses = vector::<SiteArmUse<'_>>(uses_capacity, admission).await?;
            scratch_bytes.set(scratch_bytes.get() + uses_bytes);
            if let Some(users) = users {
                for &ci in users {
                    admission.work(WorkKind::ReplayUsers).await?;
                    admission.checkpoint().await?;
                    let route = session.routes[ci]
                        .as_ref()
                        .expect("occupied route is committed");
                    let old_pi = old_indices[&(ci, point)];
                    admission.charge(size_of::<SiteArmUse<'_>>()).await?;
                    uses.push(
                        site_arm_use_admitted(
                            session,
                            ci,
                            RouteView::from(route),
                            old_pi,
                            admission,
                        )
                        .await?,
                    );
                }
            }
            admission.charge(size_of::<SiteArmUse<'_>>()).await?;
            uses.push(site_arm_use_admitted(session, pending_index, pending, pi, admission).await?);
            let (options, option_bytes) =
                junction_options_admitted(transport, point, &uses, &session.catalogs, admission)
                    .await?;
            scratch_bytes.set(scratch_bytes.get() + option_bytes);
            let key = (transport, point);
            let junction = if let Some(options) = &options {
                let selected = session
                    .junctions
                    .get(&key)
                    .filter(|prior| {
                        options.iter().any(|(component, rotation)| {
                            component == &prior.component && *rotation == prior.rotation
                        })
                    })
                    .map(|prior| (prior.component.as_str(), prior.rotation))
                    .unwrap_or_else(|| (options[0].0.as_str(), options[0].1));
                let heap_bytes = uses.len().saturating_mul(size_of::<Reverse<&str>>());
                admission.reserve(heap_bytes).await?;
                scratch_bytes.set(scratch_bytes.get() + heap_bytes);
                let mut owners = BinaryHeap::with_capacity(uses.len());
                for use_site in &uses {
                    admission.checkpoint().await?;
                    owners.push(Reverse(use_site.connection));
                }
                let mut connections = vector::<String>(uses.len(), admission).await?;
                while let Some(Reverse(id)) = owners.peek().copied() {
                    admission.charge(size_of::<String>()).await?;
                    let copied = string(id, admission).await?;
                    owners.pop();
                    connections.push(copied);
                }
                drop(owners);
                admission.release(heap_bytes).await;
                scratch_bytes.set(scratch_bytes.get() - heap_bytes);
                admission.charge(size_of::<Junction>()).await?;
                Some(Junction {
                    id: formatted_site_id("junction", transport_id, point, admission).await?,
                    component: string(selected.0, admission).await?,
                    position: point,
                    rotation: selected.1,
                    connections,
                })
            } else {
                None
            };
            drop(options);
            admission.release(option_bytes).await;
            scratch_bytes.set(scratch_bytes.get() - option_bytes);
            drop(uses);
            admission.release(uses_bytes).await;
            scratch_bytes.set(scratch_bytes.get() - uses_bytes);
            let needed_updates = updates.len() + 1;
            grow_vec_for(&mut updates, needed_updates, admission).await?;
            admission.charge(size_of::<JunctionUpdate>()).await?;
            updates.push((key, junction));
        }
        Ok(PreparedJunctionUpdates {
            updates,
            new_occupied_sites,
        })
    }
    .await;
    admission.release(scratch_bytes.get()).await;
    if result.is_err() {
        let new_bytes = admission
            .0
            .borrow()
            .reserved_bytes
            .saturating_sub(baseline_bytes);
        admission.release(new_bytes).await;
    }
    result
}

struct PreparedConnect {
    connection_index: usize,
    route_connection: String,
    new_occupied_sites: usize,
    junctions: Vec<JunctionUpdate>,
    bridges: Vec<Bridge>,
}

/// Resolve every native-invalid Connect decision while the active route is
/// still borrowed. A hard or invalid result releases only this command's new
/// preparation reservations; the replay owner retains the untouched source.
async fn prepare_connect(
    session: &Session,
    admission: &Admission,
) -> Result<PreparedConnect, RequestFailure> {
    let baseline_bytes = admission.0.borrow().reserved_bytes;
    let result = async {
        let active = session.active.as_ref().ok_or_else(|| {
            RequestFailure::Invalid(error(
                "missing-active-route",
                "active-route",
                "connect requires an active route",
            ))
        })?;
        if active.cells.last() != Some(&active.target) {
            return Err(RequestFailure::Invalid(error(
                "wrong-endpoint",
                &session.problem.connections[active.connection].id,
                "active path does not end at its selected target port",
            )));
        }
        let pending = RouteView::from(active);
        let junctions =
            prepare_junction_updates(session, active.connection, pending, admission).await?;
        let bridges =
            prepare_bridge_updates(session, active.connection, pending, admission).await?;
        let route_connection = string(
            &session.problem.connections[active.connection].id,
            admission,
        )
        .await?;
        Ok(PreparedConnect {
            connection_index: active.connection,
            route_connection,
            new_occupied_sites: junctions.new_occupied_sites,
            junctions: junctions.updates,
            bridges,
        })
    }
    .await;
    if result.is_err() {
        let new_bytes = admission
            .0
            .borrow()
            .reserved_bytes
            .saturating_sub(baseline_bytes);
        admission.release(new_bytes).await;
    }
    result
}

/// The fork is private throughout this commit phase. Map and user-vector
/// growth can move old storage over several pages, and each route cell is
/// added once after all native-invalid Connect checks have completed.
async fn commit_connect_occupied(
    fork: &mut Session,
    prepared: &PreparedConnect,
    admission: &Admission,
) -> Result<(), RequestFailure> {
    let transport = fork.problem.connections[prepared.connection_index].transport;
    let occupied = &mut fork.occupied[layer(transport)];
    grow_map_for(
        occupied,
        occupied.len().saturating_add(prepared.new_occupied_sites),
        admission,
    )
    .await?;
    let cells = &fork
        .active
        .as_ref()
        .expect("prepared Connect retains its active route")
        .cells;
    for &cell in cells {
        admission.work(WorkKind::ReplayRouteCells).await?;
        if let Some(users) = occupied.get_mut(&cell) {
            grow_vec_for(users, users.len() + 1, admission).await?;
            admission.charge(size_of::<usize>()).await?;
            users.push(prepared.connection_index);
        } else {
            admission.reserve(size_of::<usize>()).await?;
            let mut users = Vec::with_capacity(1);
            admission.charge(size_of::<usize>()).await?;
            users.push(prepared.connection_index);
            admission.charge(size_of::<(Point, Vec<usize>)>()).await?;
            occupied.insert(cell, users);
        }
    }
    Ok(())
}

/// Apply prepared component changes only to the replay owner's private fork.
/// Each old component remains alive until its nested allocation has been
/// counted, so a page can stop during replacement without losing its ledger.
async fn commit_connect_components(
    fork: &mut Session,
    prepared: &mut PreparedConnect,
    admission: &Admission,
) -> Result<(), RequestFailure> {
    let junction_vec_bytes = prepared.junctions.capacity() * size_of::<JunctionUpdate>();
    let mut junctions = std::mem::take(&mut prepared.junctions).into_iter();
    while let Some((site, replacement)) = junctions.next() {
        admission.checkpoint().await?;
        let old = if let Some(junction) = replacement {
            if !fork.junctions.contains_key(&site) {
                let needed = fork.junctions.len() + 1;
                grow_map_for(&mut fork.junctions, needed, admission).await?;
            }
            admission
                .charge(size_of::<(
                    (aic_layout::logistics::TransportKind, Point),
                    Junction,
                )>())
                .await?;
            fork.junctions.insert(site, junction)
        } else {
            fork.junctions.remove(&site)
        };
        if let Some(old) = old {
            let bytes = junction_heap_bytes(&old, admission).await?;
            drop(old);
            admission.release(bytes).await;
        }
    }
    drop(junctions);
    admission.release(junction_vec_bytes).await;

    let transport = fork.problem.connections[prepared.connection_index].transport;
    let bridge_vec_bytes = prepared.bridges.capacity() * size_of::<Bridge>();
    let mut bridges = std::mem::take(&mut prepared.bridges).into_iter();
    while let Some(bridge) = bridges.next() {
        admission.checkpoint().await?;
        let site = (transport, bridge.position);
        if !fork.bridges.contains_key(&site) {
            let needed = fork.bridges.len() + 1;
            grow_map_for(&mut fork.bridges, needed, admission).await?;
        }
        admission
            .charge(size_of::<(
                (aic_layout::logistics::TransportKind, Point),
                Bridge,
            )>())
            .await?;
        let old = fork.bridges.insert(site, bridge);
        if let Some(old) = old {
            let bytes = bridge_heap_bytes(&old, admission).await?;
            drop(old);
            admission.release(bytes).await;
        }
    }
    drop(bridges);
    admission.release(bridge_vec_bytes).await;
    Ok(())
}

/// Finish the private Connect after occupancy and component updates. The
/// active cell allocation becomes route storage; only the bridge-reservation
/// allocation is discarded. Port IDs are copied for map keys under admission.
async fn commit_connect_ports_and_route(
    fork: &mut Session,
    prepared: &mut PreparedConnect,
    admission: &Admission,
) -> Result<(), RequestFailure> {
    let connection = &fork.problem.connections[prepared.connection_index];
    let active = fork
        .active
        .as_ref()
        .expect("prepared Connect retains its active route");
    for (endpoint, port, direction) in [
        (
            &connection.source,
            active.source_port.as_deref(),
            FacilityPortDirection::Output,
        ),
        (
            &connection.target,
            active.target_port.as_deref(),
            FacilityPortDirection::Input,
        ),
    ] {
        let (Some(&block), Some(port)) = (fork.blocks.get(endpoint), port) else {
            continue;
        };
        admission.checkpoint().await?;
        let key = (block, string(port, admission).await?);
        if let Some(used) = fork.used_ports.get_mut(&key) {
            used.rate = used
                .rate
                .checked_add(connection.rate)
                .expect("begin-route checked shared port rate");
            let key_bytes = key.1.len();
            drop(key);
            admission.release(key_bytes).await;
        } else {
            let needed = fork.used_ports.len() + 1;
            grow_map_for(&mut fork.used_ports, needed, admission).await?;
            let commodity = string(&connection.commodity, admission).await?;
            admission
                .charge(size_of::<((usize, String), UsedPort)>())
                .await?;
            fork.used_ports.insert(
                key,
                UsedPort {
                    commodity,
                    transport: connection.transport,
                    direction,
                    rate: connection.rate,
                },
            );
        }
    }

    let mut active = fork
        .active
        .take()
        .expect("prepared Connect has an active route");
    let bridge_vec_bytes = active.bridge_reservations.capacity() * size_of::<BridgeReservation>();
    while let Some(reservation) = active.bridge_reservations.pop() {
        admission.checkpoint().await?;
        let component_bytes = reservation.component.len();
        drop(reservation);
        admission.release(component_bytes).await;
    }
    drop(active.bridge_reservations);
    admission.release(bridge_vec_bytes).await;

    let route = Route {
        connection: std::mem::take(&mut prepared.route_connection),
        source_port: active.source_port,
        target_port: active.target_port,
        cells: active.cells,
    };
    for &cell in &route.cells {
        admission.checkpoint().await?;
        fork.active_cells.remove(&cell);
    }
    let target = active.target;
    fork.routes[prepared.connection_index] = Some(route);
    fork.focus = Some(Focus::Route {
        connection: prepared.connection_index,
        endpoint: target,
        target,
        active: false,
    });
    Ok(())
}

async fn commit_prepared_connect(
    fork: &mut Session,
    prepared: &mut PreparedConnect,
    admission: &Admission,
) -> Result<(), RequestFailure> {
    commit_connect_occupied(fork, prepared, admission).await?;
    commit_connect_components(fork, prepared, admission).await?;
    commit_connect_ports_and_route(fork, prepared, admission).await?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ReplayCommandOutcome {
    pub(super) turn: u64,
    pub(super) revision: u64,
    pub(super) progress_before: f64,
    pub(super) progress_after: f64,
    pub(super) terminal: bool,
}

enum PreparedRouteCommand {
    Begin(PreparedBeginRoute),
    Extend(PreparedExtend),
    Bridge(PreparedBridge),
    Connect(PreparedConnect),
}

fn outcomes_after_failure(
    outcomes: Vec<ReplayCommandOutcome>,
    failure: &RequestFailure,
) -> Vec<ReplayCommandOutcome> {
    if matches!(failure, RequestFailure::Stopped(_)) {
        Vec::new()
    } else {
        outcomes
    }
}

/// Replay route commands on one exclusively owned fork. The caller owns the
/// original source and discards this entire fork after any failure. Outcomes
/// describe the accepted prefix without constructing public Step snapshots.
async fn replay_route_commands(
    fork: &mut Session,
    commands: &[Command],
    admission: &Admission,
) -> (Vec<ReplayCommandOutcome>, Result<(), RequestFailure>) {
    let total = fork.poses.len() + fork.terminal_poses.len() + fork.routes.len();
    let mut completed = fork.poses.iter().filter(|pose| pose.is_some()).count()
        + fork
            .terminal_poses
            .iter()
            .filter(|pose| pose.is_some())
            .count()
        + fork.routes.iter().filter(|route| route.is_some()).count();
    let mut outcomes = Vec::new();
    let mut ledger = ActiveStorageLedger::for_fork(fork);
    for command in commands {
        if completed == total || fork.final_failure.is_some() || fork.turns >= fork.limits.max_turns
        {
            return (
                outcomes,
                Err(RequestFailure::Invalid(error(
                    "episode-closed",
                    "episode",
                    "no command is accepted after completion, failure or turn exhaustion",
                ))),
            );
        }
        let before = completed as f64 / total.max(1) as f64;
        let prepared = match command {
            Command::BeginRoute {
                connection,
                source_port,
                target_port,
            } => prepare_begin_route(
                fork,
                connection,
                source_port.as_deref(),
                target_port.as_deref(),
                admission,
            )
            .await
            .map(PreparedRouteCommand::Begin),
            Command::Extend { direction, length } => {
                prepare_extend(fork, *direction, *length, admission)
                    .await
                    .map(PreparedRouteCommand::Extend)
            }
            Command::ReserveBridge {
                position,
                component,
                rotation,
                crossed_axis,
            } => prepare_reserve_bridge(
                fork,
                *position,
                component,
                *rotation,
                *crossed_axis,
                admission,
            )
            .await
            .map(PreparedRouteCommand::Bridge),
            Command::Connect => prepare_connect(fork, admission)
                .await
                .map(PreparedRouteCommand::Connect),
            _ => Err(RequestFailure::Invalid(error(
                "route-candidate-replay",
                "route-candidate",
                "internal route replay received a non-route command",
            ))),
        };
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(failure) => {
                return (outcomes_after_failure(outcomes, &failure), Err(failure));
            }
        };
        let needed = outcomes.len() + 1;
        if let Err(failure) = grow_vec_for(&mut outcomes, needed, admission).await {
            return (Vec::new(), Err(failure));
        }
        if let Err(failure) = admission.charge(size_of::<ReplayCommandOutcome>()).await {
            return (Vec::new(), Err(failure));
        }
        outcomes.push(ReplayCommandOutcome {
            turn: fork.turns + 1,
            revision: fork.revision + 1,
            progress_before: before,
            progress_after: before,
            terminal: false,
        });
        let result = match prepared {
            PreparedRouteCommand::Begin(prepared) => {
                commit_begin_route(fork, prepared, &mut ledger);
                Ok(())
            }
            PreparedRouteCommand::Extend(prepared) => {
                commit_extend(fork, prepared, &mut ledger, admission).await
            }
            PreparedRouteCommand::Bridge(prepared) => {
                commit_reserve_bridge(fork, prepared, &mut ledger, admission).await
            }
            PreparedRouteCommand::Connect(mut prepared) => {
                let result = commit_prepared_connect(fork, &mut prepared, admission).await;
                if result.is_ok() {
                    ledger = ActiveStorageLedger::for_fork(fork);
                    completed += 1;
                }
                result
            }
        };
        if let Err(failure) = result {
            outcomes.pop();
            return (outcomes_after_failure(outcomes, &failure), Err(failure));
        }
        fork.turns += 1;
        fork.revision += 1;
        let outcome = outcomes.last_mut().expect("prepared outcome was written");
        outcome.progress_after = completed as f64 / total.max(1) as f64;
        outcome.terminal = completed == total || fork.turns >= fork.limits.max_turns;
    }
    (outcomes, Ok(()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CursorRequest {
    Visit,
    Reserve(usize),
}

/// Bridge the existing completed-layout cursor to the replay admission loop.
/// The cursor's partial layout remains inside this future across page stops.
async fn materialize_admitted(
    completed: &Arc<Session>,
    admission: &Admission,
) -> Result<(Layout, usize), RequestFailure> {
    let mut cursor = CompletedLayoutCursor::new(completed);
    let grant = Cell::new(None);
    loop {
        let requested = Cell::new(None);
        let released = Cell::new(0usize);
        let result = cursor.advance(
            |_count| {
                if grant.get() == Some(CursorRequest::Visit) {
                    grant.set(None);
                    1
                } else {
                    requested.set(Some(CursorRequest::Visit));
                    0
                }
            },
            |bytes| {
                if grant.get() == Some(CursorRequest::Reserve(bytes)) {
                    grant.set(None);
                    MaterializationReservation::Granted
                } else {
                    requested.set(Some(CursorRequest::Reserve(bytes)));
                    MaterializationReservation::Retry
                }
            },
            |bytes| released.set(released.get().saturating_add(bytes)),
        );
        admission.release(released.get()).await;
        match result.map_err(RequestFailure::Invalid)? {
            MaterializationAdvance::Complete {
                layout,
                reserved_bytes,
            } => return Ok((layout, reserved_bytes)),
            MaterializationAdvance::Pending => {
                match requested.get().expect("cursor requested admission") {
                    CursorRequest::Visit => {
                        admission.work(WorkKind::ValidationRecords).await?;
                        grant.set(Some(CursorRequest::Visit));
                    }
                    CursorRequest::Reserve(bytes) => {
                        admission.reserve(bytes).await?;
                        grant.set(Some(CursorRequest::Reserve(bytes)));
                    }
                }
            }
        }
    }
}

async fn validate_admitted(
    layout: Arc<Layout>,
    catalogs: Arc<Catalogs>,
    admission: &Admission,
) -> Result<Metrics, RequestFailure> {
    let mut cursor = ValidationCursor::new(layout, catalogs);
    let grant = Cell::new(None);
    loop {
        let requested = Cell::new(None);
        let released = Cell::new(0usize);
        let result = cursor.advance(
            || {
                if grant.get() == Some(CursorRequest::Visit) {
                    grant.set(None);
                    ValidationVisit::Granted
                } else {
                    requested.set(Some(CursorRequest::Visit));
                    ValidationVisit::Retry
                }
            },
            |bytes| {
                if grant.get() == Some(CursorRequest::Reserve(bytes)) {
                    grant.set(None);
                    ValidationReservation::Granted
                } else {
                    requested.set(Some(CursorRequest::Reserve(bytes)));
                    ValidationReservation::Retry
                }
            },
            |bytes| released.set(released.get().saturating_add(bytes)),
        );
        admission.release(released.get()).await;
        match result {
            ValidationAdvance::Complete(Ok(metrics)) => return Ok(metrics),
            ValidationAdvance::Complete(Err(failure)) => {
                return Err(RequestFailure::Invalid(error(
                    "final-validation",
                    failure.entity.to_string(),
                    failure.message,
                )));
            }
            ValidationAdvance::Pending => {
                match requested.get().expect("validator requested admission") {
                    CursorRequest::Visit => {
                        admission.work(WorkKind::ValidationRecords).await?;
                        grant.set(Some(CursorRequest::Visit));
                    }
                    CursorRequest::Reserve(bytes) => {
                        admission.reserve(bytes).await?;
                        grant.set(Some(CursorRequest::Reserve(bytes)));
                    }
                }
            }
        }
    }
}

/// A completed fork is frozen while its layout and independent validator run.
/// Only the uniquely owned fork receives Metrics before it can be returned.
async fn validate_completed_fork(
    fork: Session,
    admission: &Admission,
) -> Result<Session, RequestFailure> {
    let arc_header_bytes = 2 * size_of::<usize>();
    admission.reserve(arc_header_bytes).await?;
    let frozen = Arc::new(fork);
    let (layout, layout_bytes) = materialize_admitted(&frozen, admission).await?;
    admission.reserve(arc_header_bytes).await?;
    let layout = Arc::new(layout);
    let validation = validate_admitted(layout.clone(), frozen.catalogs.clone(), admission).await;
    drop(layout);
    admission.release(layout_bytes + arc_header_bytes).await;
    let metrics = validation?;
    let mut child = match Arc::try_unwrap(frozen) {
        Ok(child) => child,
        Err(_) => unreachable!("completed replay holds the unique fork"),
    };
    admission.release(arc_header_bytes).await;
    child.metrics = Some(metrics);
    Ok(child)
}

enum CommandOwner<'a> {
    Borrowed(&'a [Command]),
    Owned(Vec<Command>),
}

impl CommandOwner<'_> {
    fn commands(&self) -> &[Command] {
        match self {
            Self::Borrowed(commands) => commands,
            Self::Owned(commands) => commands,
        }
    }
}

async fn replay_owned_source<'a>(
    source: SourceOwner<'a>,
    commands: CommandOwner<'a>,
    admission: Admission,
) -> (
    SourceOwner<'a>,
    Result<(Session, Vec<ReplayCommandOutcome>), (RequestFailure, Vec<ReplayCommandOutcome>)>,
) {
    let mut fork = match copy_session(source.session(), admission.clone()).await {
        Ok(fork) => fork,
        Err(failure) => return (source, Err((failure, Vec::new()))),
    };
    let (outcomes, status) =
        replay_route_commands(&mut fork, commands.commands(), &admission).await;
    if let Err(failure) = status {
        return (source, Err((failure, outcomes)));
    }
    if fork.is_complete() {
        fork = match validate_completed_fork(fork, &admission).await {
            Ok(child) => child,
            Err(failure) => {
                let outcomes = outcomes_after_failure(outcomes, &failure);
                return (source, Err((failure, outcomes)));
            }
        };
    }
    (source, Ok((fork, outcomes)))
}

/// Owns the source, partial fork, admitted command work and terminal validator
/// in one retained future. A soft stop keeps the exact future; hard failure
/// drops its private state before returning the untouched original source.
pub(super) struct NativeReplayProgress<'a> {
    future: Option<
        Pin<
            Box<
                dyn Future<
                        Output = (
                            SourceOwner<'a>,
                            Result<
                                (Session, Vec<ReplayCommandOutcome>),
                                (RequestFailure, Vec<ReplayCommandOutcome>),
                            >,
                        ),
                    > + 'a,
            >,
        >,
    >,
    admission: Admission,
}

pub(super) enum ReplayAdvance<'a> {
    Pending(AllowanceStop),
    Complete(
        SourceOwner<'a>,
        Result<
            (Session, Vec<ReplayCommandOutcome>, usize),
            (RequestFailure, Vec<ReplayCommandOutcome>, usize),
        >,
        ReplayWorkDelta,
    ),
}

impl<'a> NativeReplayProgress<'a> {
    pub(super) fn new_borrowed(source: &'a Session, commands: &'a [Command]) -> Self {
        Self::new_owner(
            SourceOwner::Borrowed(source),
            CommandOwner::Borrowed(commands),
        )
    }

    pub(super) fn new_shared(source: Arc<Session>, commands: Vec<Command>) -> Self {
        Self::new_owner(SourceOwner::Shared(source), CommandOwner::Owned(commands))
    }

    pub(super) fn new_owned(source: Session, commands: Vec<Command>) -> Self {
        Self::new_owner(SourceOwner::Owned(source), CommandOwner::Owned(commands))
    }

    fn new_owner(source: SourceOwner<'a>, commands: CommandOwner<'a>) -> Self {
        let admission = Admission::default();
        let future = Box::pin(replay_owned_source(source, commands, admission.clone()));
        Self {
            future: Some(future),
            admission,
        }
    }

    pub(super) fn reserved_bytes(&self) -> usize {
        self.admission.0.borrow().reserved_bytes
    }

    pub(super) fn cancel(mut self, allowance: &mut RequestAllowance) {
        self.future = None;
        let bytes = std::mem::take(&mut self.admission.0.borrow_mut().reserved_bytes);
        allowance.release_peak(bytes);
    }

    pub(super) fn advance(
        &mut self,
        allowance: &mut RequestAllowance,
        pinned_external_bytes: usize,
    ) -> ReplayAdvance<'a> {
        match poll_admitted(
            &mut self.future,
            &self.admission,
            allowance,
            pinned_external_bytes,
        ) {
            AdmittedAdvance::Pending(stop) => ReplayAdvance::Pending(stop),
            AdmittedAdvance::Complete((source, result)) => {
                let mut control = self.admission.0.borrow_mut();
                let bytes = std::mem::take(&mut control.reserved_bytes);
                let work = control.work;
                drop(control);
                match result {
                    Ok((child, outcomes)) => {
                        ReplayAdvance::Complete(source, Ok((child, outcomes, bytes)), work)
                    }
                    Err((failure, outcomes)) => {
                        let retained_bytes = outcomes
                            .capacity()
                            .saturating_mul(size_of::<ReplayCommandOutcome>());
                        debug_assert!(retained_bytes <= bytes);
                        allowance.release_peak(bytes.saturating_sub(retained_bytes));
                        ReplayAdvance::Complete(
                            source,
                            Err((failure, outcomes, retained_bytes)),
                            work,
                        )
                    }
                }
            }
        }
    }
}

struct PreparedBridge {
    position: Point,
    component: String,
    rotation: i64,
    crossed_axis: BridgeAxis,
    direction: usize,
}

/// Ordinary commands and retained replay drive the same bridge rule. The
/// ordinary path has no page or copy allowance; its existing replay meter and
/// deadline still account for every visited source record.
pub(super) fn reserve_bridge_native(
    session: &mut Session,
    position: Point,
    component: &str,
    rotation: i64,
    crossed_axis: BridgeAxis,
) -> Result<(), Diagnostic> {
    let admission = Admission::default();
    let prepared = {
        let mut future = Box::pin(prepare_reserve_bridge(
            session,
            position,
            component,
            rotation,
            crossed_axis,
            &admission,
        ));
        let mut context = Context::from_waker(Waker::noop());
        loop {
            let request = admission.0.borrow_mut().request.take();
            if let Some(request) = request {
                let result = match request {
                    AdmissionRequest::ChargePrefix { len, .. } => Ok(AdmissionGrant::Items(len)),
                    AdmissionRequest::Work(kind) => {
                        let replay_kind = match kind {
                            WorkKind::ReplayRouteCells => ReplayWork::RouteCells,
                            WorkKind::ReplayUsers => ReplayWork::Users,
                            WorkKind::CatalogVisits => ReplayWork::Catalog,
                            _ => unreachable!("bridge preparation only visits native replay work"),
                        };
                        session
                            .charge_replay(replay_kind, 1)
                            .map(|_| AdmissionGrant::Done)
                            .map_err(RequestFailure::Invalid)
                    }
                    AdmissionRequest::Checkpoint => session
                        .check_replay_deadline()
                        .map(|_| AdmissionGrant::Done)
                        .map_err(RequestFailure::Invalid),
                    AdmissionRequest::Reserve(bytes) => {
                        admission.0.borrow_mut().reserved_bytes += bytes;
                        Ok(AdmissionGrant::Done)
                    }
                    AdmissionRequest::Release(bytes) => {
                        admission.0.borrow_mut().reserved_bytes -= bytes;
                        Ok(AdmissionGrant::Done)
                    }
                    AdmissionRequest::ChargeRemainder(_)
                    | AdmissionRequest::PlanGrowth(_)
                    | AdmissionRequest::PlanVecGrowth(_)
                    | AdmissionRequest::PlanMapGrowth(_) => {
                        unreachable!("unbounded bridge copy cannot need a remainder or growth")
                    }
                };
                admission.0.borrow_mut().response = Some(result);
                continue;
            }
            match future.as_mut().poll(&mut context) {
                Poll::Ready(result) => break result,
                Poll::Pending => debug_assert!(admission.0.borrow().request.is_some()),
            }
        }
    };
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(RequestFailure::Invalid(diagnostic)) => return Err(diagnostic),
        Err(_) => unreachable!("ordinary native bridge admission only returns diagnostics"),
    };
    session
        .active
        .as_mut()
        .expect("prepared bridge has an active route")
        .bridge_reservations
        .push(BridgeReservation {
            position: prepared.position,
            component: prepared.component,
            rotation: prepared.rotation,
            crossed_axis: prepared.crossed_axis,
            direction: prepared.direction,
        });
    Ok(())
}

/// Resolve every native-invalid bridge condition before a private fork takes
/// ownership of the reservation. Source route and catalog visits retain their
/// exact iterator positions across an admission stop.
async fn prepare_reserve_bridge(
    session: &Session,
    position: Point,
    component: &str,
    rotation: i64,
    crossed_axis: BridgeAxis,
    admission: &Admission,
) -> Result<PreparedBridge, RequestFailure> {
    let active = session.active.as_ref().ok_or_else(|| {
        RequestFailure::Invalid(error(
            "missing-active-route",
            "active-route",
            "bridge reservation requires an active route",
        ))
    })?;
    let obligation = &session.problem.connections[active.connection];
    let previous = *active.cells.last().expect("active route has a source");
    let Some(direction) = point_direction(previous, position) else {
        return Err(RequestFailure::Invalid(error(
            "bridge-reservation",
            &obligation.id,
            "bridge must be reserved immediately before entering its adjacent cell",
        )));
    };
    let expected_axis = if direction % 2 == 0 {
        BridgeAxis::Vertical
    } else {
        BridgeAxis::Horizontal
    };
    if crossed_axis != expected_axis
        || session.bodies.contains(&position)
        || session.terminal_at.contains_key(&position)
        || session.active_cells.contains(&position)
        || session
            .junctions
            .contains_key(&(obligation.transport, position))
    {
        return Err(RequestFailure::Invalid(error(
            "bridge-reservation",
            &obligation.id,
            "bridge axis, site or reservation conflicts with occupied geometry",
        )));
    }
    for reserved in &active.bridge_reservations {
        admission.work(WorkKind::ReplayUsers).await?;
        if reserved.position == position {
            return Err(RequestFailure::Invalid(error(
                "bridge-reservation",
                &obligation.id,
                "bridge axis, site or reservation conflicts with occupied geometry",
            )));
        }
    }
    let users = session.occupied[layer(obligation.transport)]
        .get(&position)
        .ok_or_else(|| {
            RequestFailure::Invalid(error(
                "bridge-reservation",
                &obligation.id,
                "bridge site has no committed carrier axis",
            ))
        })?;

    if let Some(installed) = session.bridges.get(&(obligation.transport, position)) {
        let channel = &installed.channels[direction % 2];
        let definition = session
            .catalogs
            .components
            .component(&installed.component)
            .expect("installed bridge has a catalog component");
        let mut load = obligation.rate;
        let mut matches_axis = channel.commodity == obligation.commodity
            && installed.component == component
            && installed.rotation == rotation;
        for owner in &channel.connections {
            admission.work(WorkKind::ReplayUsers).await?;
            let ci = session.connections[owner];
            let route = session.routes[ci]
                .as_ref()
                .expect("bridge owner route is committed");
            let pi = find_route_cell(route, position, admission)
                .await?
                .expect("bridge owner visits its component");
            matches_axis &= pi > 0
                && pi + 1 < route.cells.len()
                && point_direction(route.cells[pi - 1], position) == Some(direction)
                && point_direction(position, route.cells[pi + 1]) == Some(direction);
            load = load
                .checked_add(session.problem.connections[ci].rate)
                .map_err(|_| {
                    RequestFailure::Invalid(error(
                        "bridge-reservation",
                        &obligation.id,
                        "bridge axis rate overflows",
                    ))
                })?;
        }
        let line = session.catalogs.transports.capacity(obligation.transport);
        if !matches_axis
            || !within_capacity(
                load,
                definition.capacity.quantity,
                definition.capacity.duration_ms,
            )
            || !within_capacity(load, line.quantity, line.duration_ms)
        {
            return Err(RequestFailure::Invalid(error(
                "bridge-reservation",
                &obligation.id,
                "installed bridge axis has incompatible flow or insufficient residual capacity",
            )));
        }
    } else {
        let carrier_commodity = &session.problem.connections[users[0]].commodity;
        let mut carrier_direction = None;
        let mut carrier_rate = Rate::zero();
        for &ci in users {
            admission.work(WorkKind::ReplayUsers).await?;
            let existing = &session.problem.connections[ci];
            let route = session.routes[ci]
                .as_ref()
                .expect("occupied route is committed");
            let Some(pi) = find_route_cell(route, position, admission).await? else {
                return Err(RequestFailure::Invalid(error(
                    "bridge-reservation",
                    &obligation.id,
                    "carrier route does not visit the bridge site",
                )));
            };
            let heading = if pi > 0 && pi + 1 < route.cells.len() {
                point_direction(route.cells[pi - 1], position)
            } else {
                None
            };
            if existing.commodity != *carrier_commodity
                || heading.is_none()
                || point_direction(position, route.cells[pi + 1]) != heading
                || carrier_direction.is_some_and(|old| heading != Some(old))
            {
                return Err(RequestFailure::Invalid(error(
                    "bridge-reservation",
                    &obligation.id,
                    "carrier routes must form one same-commodity straight directed axis",
                )));
            }
            carrier_direction = heading;
            carrier_rate = carrier_rate.checked_add(existing.rate).map_err(|_| {
                RequestFailure::Invalid(error(
                    "bridge-reservation",
                    &obligation.id,
                    "carrier axis rate overflows",
                ))
            })?;
        }
        let legal = if let Some(definition) = session.catalogs.components.component(component) {
            if let Some(carrier_direction) = carrier_direction {
                let fixed = definition.kind == LogisticsComponentKind::Bridge
                    && definition.transport == obligation.transport
                    && carrier_direction % 2 != direction % 2
                    && definition.footprint.width == 1
                    && definition.footprint.height == 1
                    && within_capacity(
                        carrier_rate,
                        definition.capacity.quantity,
                        definition.capacity.duration_ms,
                    )
                    && within_capacity(
                        obligation.rate,
                        definition.capacity.quantity,
                        definition.capacity.duration_ms,
                    );
                let mut allowed_rotation = false;
                if fixed {
                    for &allowed in &definition.allowed_rotations {
                        admission.work(WorkKind::CatalogVisits).await?;
                        if allowed == rotation {
                            allowed_rotation = true;
                            break;
                        }
                    }
                }
                let mut arms_fit = true;
                if fixed && allowed_rotation {
                    let rotated = |arm| (cardinal_index(arm) + rotation as usize / 90) % 4;
                    for heading in [carrier_direction, direction] {
                        let mut input_fit = false;
                        for &arm in &definition.input_directions {
                            admission.work(WorkKind::CatalogVisits).await?;
                            if rotated(arm) == (heading + 2) % 4 {
                                input_fit = true;
                                break;
                            }
                        }
                        let mut output_fit = false;
                        if input_fit {
                            for &arm in &definition.output_directions {
                                admission.work(WorkKind::CatalogVisits).await?;
                                if rotated(arm) == heading {
                                    output_fit = true;
                                    break;
                                }
                            }
                        }
                        if !input_fit || !output_fit {
                            arms_fit = false;
                            break;
                        }
                    }
                }
                fixed && allowed_rotation && arms_fit
            } else {
                false
            }
        } else {
            false
        };
        if !legal {
            return Err(RequestFailure::Invalid(error(
                "bridge-reservation",
                &obligation.id,
                "catalog bridge does not fit the carrier, new route, rotation or capacity",
            )));
        }
    }
    Ok(PreparedBridge {
        position,
        component: string(component, admission).await?,
        rotation,
        crossed_axis,
        direction,
    })
}

struct PreparedExtend {
    cells: Vec<Point>,
    reserved_bytes: usize,
}

/// Fork-owned active allocations already paid for by the retained replay
/// allowance. Refresh after a route begins or aborts; update after each
/// committed extension or bridge reservation without rescanning route content.
#[derive(Clone, Copy)]
struct ActiveStorageLedger {
    vec_bytes: usize,
    set_bytes: usize,
    bridge_bytes: usize,
}

impl ActiveStorageLedger {
    fn for_fork(fork: &Session) -> Self {
        Self {
            vec_bytes: fork
                .active
                .as_ref()
                .map_or(0, |active| active.cells.capacity() * size_of::<Point>()),
            set_bytes: hash_bucket_count(fork.active_cells.capacity()) * (size_of::<Point>() + 1),
            bridge_bytes: fork.active.as_ref().map_or(0, |active| {
                active.bridge_reservations.capacity() * size_of::<BridgeReservation>()
            }),
        }
    }
}

/// This runs only on the private replay fork. A page may interrupt the old
/// reservation transfer, but the old records and replacement remain owned by
/// this future until the one-time vector swap completes.
async fn commit_reserve_bridge(
    fork: &mut Session,
    prepared: PreparedBridge,
    ledger: &mut ActiveStorageLedger,
    admission: &Admission,
) -> Result<(), RequestFailure> {
    let active = fork
        .active
        .as_ref()
        .expect("prepared bridge has an active route");
    let needed = active.bridge_reservations.len() + 1;
    if active.bridge_reservations.capacity() < needed {
        let capacity = admission
            .reserve_vec_growth(VecGrowthSpec {
                needed,
                old_capacity: active.bridge_reservations.capacity(),
                element_bytes: size_of::<BridgeReservation>(),
            })
            .await?;
        let mut replacement = Vec::with_capacity(capacity);
        let mut old = std::mem::take(
            &mut fork
                .active
                .as_mut()
                .expect("prepared bridge has an active route")
                .bridge_reservations,
        )
        .into_iter();
        while old.len() > 0 {
            let count = old.len().min(64);
            let costs = [size_of::<BridgeReservation>(); 64];
            let granted = admission.charge_prefix(&costs[..count]).await?;
            for _ in 0..granted {
                replacement.push(old.next().expect("granted bridge reservation"));
            }
        }
        drop(old);
        admission.charge(size_of::<BridgeReservation>()).await?;
        replacement.push(BridgeReservation {
            position: prepared.position,
            component: prepared.component,
            rotation: prepared.rotation,
            crossed_axis: prepared.crossed_axis,
            direction: prepared.direction,
        });
        fork.active
            .as_mut()
            .expect("active route retained")
            .bridge_reservations = replacement;
        let old_bytes = ledger.bridge_bytes;
        ledger.bridge_bytes = capacity * size_of::<BridgeReservation>();
        admission.release(old_bytes).await;
    } else {
        admission.charge(size_of::<BridgeReservation>()).await?;
        fork.active
            .as_mut()
            .expect("active route retained")
            .bridge_reservations
            .push(BridgeReservation {
                position: prepared.position,
                component: prepared.component,
                rotation: prepared.rotation,
                crossed_axis: prepared.crossed_axis,
                direction: prepared.direction,
            });
    }
    Ok(())
}

/// Finish all native-invalid extension checks before the private fork changes.
/// The returned cells and their reservation stay owned by the replay future
/// until its retained commit phase appends them once.
async fn prepare_extend(
    session: &Session,
    direction: Direction,
    length: i64,
    admission: &Admission,
) -> Result<PreparedExtend, RequestFailure> {
    if !(1..=session.limits.max_extension).contains(&length) {
        return Err(RequestFailure::Invalid(error(
            "extension-length",
            "active-route",
            "straight extension length is outside the configured action domain",
        )));
    }
    let active = session.active.as_ref().ok_or_else(|| {
        RequestFailure::Invalid(error(
            "missing-active-route",
            "active-route",
            "extend requires an active route",
        ))
    })?;
    let connection = active.connection;
    let (dx, dy) = direction.offset();
    let heading = match direction {
        Direction::East => 0,
        Direction::South => 1,
        Direction::West => 2,
        Direction::North => 3,
    };
    let count = length as usize;
    let cells_bytes = count.saturating_mul(size_of::<Point>());
    let set_bytes = hash_bucket_count(count).saturating_mul(size_of::<Point>() + 1);
    admission.reserve(cells_bytes).await?;
    if let Err(failure) = admission.reserve(set_bytes).await {
        admission.release(cells_bytes).await;
        return Err(failure);
    }
    let mut added = Vec::with_capacity(count);
    let mut new_cells = HashSet::with_capacity(count);
    let mut point = *active.cells.last().expect("active route has a source");
    let scanned: Result<(), RequestFailure> = async {
        for _ in 0..count {
            admission.work(WorkKind::ReplayRouteCells).await?;
            let (next, needs_shared_check) = session
                .extension_step_geometry(point, &new_cells, connection, dx, dy, heading)
                .map_err(RequestFailure::Invalid)?;
            if needs_shared_check
                && !shared_cell_available(session, next, connection, admission).await?
            {
                return Err(RequestFailure::Invalid(
                    Session::extension_intersection_error(),
                ));
            }
            new_cells.insert(next);
            added.push(next);
            point = next;
        }
        Ok(())
    }
    .await;
    if let Err(failure) = scanned {
        drop(new_cells);
        drop(added);
        admission
            .release(set_bytes.saturating_add(cells_bytes))
            .await;
        return Err(failure);
    }
    drop(new_cells);
    admission.release(set_bytes).await;
    Ok(PreparedExtend {
        cells: added,
        reserved_bytes: cells_bytes,
    })
}

/// Only a private fork enters this phase. Legality was settled by
/// `prepare_extend`; page stops may leave the two private containers at
/// different positions, but no child is published before both finish.
async fn commit_extend(
    fork: &mut Session,
    prepared: PreparedExtend,
    ledger: &mut ActiveStorageLedger,
    admission: &Admission,
) -> Result<(), RequestFailure> {
    let active = fork
        .active
        .as_ref()
        .expect("prepared extension has an active route");
    let connection = active.connection;
    let target = active.target;
    let needed_cells = active.cells.len() + prepared.cells.len();
    let spec = GrowthSpec {
        needed_cells,
        old_vec_capacity: active.cells.capacity(),
        old_set_capacity: fork.active_cells.capacity(),
        old_vec_bytes: ledger.vec_bytes,
        old_set_bytes: ledger.set_bytes,
    };
    let plan = admission.reserve_joint_growth(spec).await?;

    async fn append_vec(
        destination: &mut Vec<Point>,
        added: &[Point],
        admission: &Admission,
    ) -> Result<(), RequestFailure> {
        for &point in added {
            admission.checkpoint().await?;
            destination.push(point);
        }
        Ok(())
    }

    async fn append_set(
        destination: &mut HashSet<Point>,
        added: &[Point],
        admission: &Admission,
    ) -> Result<(), RequestFailure> {
        for &point in added {
            admission.checkpoint().await?;
            destination.insert(point);
        }
        Ok(())
    }

    async fn commit_vec(
        fork: &mut Session,
        added: &[Point],
        plan: JointGrowthPlan,
        admission: &Admission,
    ) -> Result<(), RequestFailure> {
        let cells = &mut fork.active.as_mut().expect("active route retained").cells;
        if cells.capacity() < cells.len() + added.len() {
            let mut replacement = Vec::with_capacity(plan.vec_capacity);
            while replacement.len() < cells.len() {
                let count = (cells.len() - replacement.len()).min(64);
                let costs = [size_of::<Point>(); 64];
                let granted = admission.charge_prefix(&costs[..count]).await?;
                let start = replacement.len();
                replacement.extend_from_slice(&cells[start..start + granted]);
            }
            append_vec(&mut replacement, added, admission).await?;
            *cells = replacement;
        } else {
            append_vec(cells, added, admission).await?;
        }
        Ok(())
    }

    async fn commit_set(
        fork: &mut Session,
        added: &[Point],
        plan: JointGrowthPlan,
        admission: &Admission,
    ) -> Result<(), RequestFailure> {
        let cells = &mut fork.active_cells;
        if cells.capacity() < cells.len() + added.len() {
            let mut replacement = HashSet::with_capacity(plan.set_capacity);
            let mut old = cells.iter();
            while old.len() > 0 {
                let count = old.len().min(64);
                let costs = [size_of::<Point>(); 64];
                let granted = admission.charge_prefix(&costs[..count]).await?;
                for _ in 0..granted {
                    replacement.insert(*old.next().expect("granted old cell"));
                }
            }
            append_set(&mut replacement, added, admission).await?;
            *cells = replacement;
        } else {
            append_set(cells, added, admission).await?;
        }
        Ok(())
    }

    match plan.order {
        GrowthOrder::VecThenSet => {
            commit_vec(fork, &prepared.cells, plan, admission).await?;
            commit_set(fork, &prepared.cells, plan, admission).await?;
        }
        GrowthOrder::SetThenVec => {
            commit_set(fork, &prepared.cells, plan, admission).await?;
            commit_vec(fork, &prepared.cells, plan, admission).await?;
        }
    }

    let new_vec_bytes = fork.active.as_ref().unwrap().cells.capacity() * size_of::<Point>();
    let new_set_bytes = plan
        .set_buckets
        .max(hash_bucket_count(fork.active_cells.capacity()))
        * (size_of::<Point>() + 1);
    let retained_growth = new_vec_bytes - ledger.vec_bytes + new_set_bytes - ledger.set_bytes;
    ledger.vec_bytes = new_vec_bytes;
    ledger.set_bytes = new_set_bytes;
    let endpoint = *prepared.cells.last().expect("extension is nonempty");
    drop(prepared.cells);
    admission
        .release(prepared.reserved_bytes + plan.escrow_bytes - retained_growth)
        .await;
    fork.focus = Some(Focus::Route {
        connection,
        endpoint,
        target,
        active: true,
    });
    Ok(())
}

async fn string(source: &str, admission: &Admission) -> Result<String, RequestFailure> {
    admission.reserve(source.len()).await?;
    let mut copied = String::with_capacity(source.len());
    let mut source_chars = source.chars();
    while !source_chars.as_str().is_empty() {
        let mut costs = [0; 64];
        let mut count = 0;
        for ch in source_chars.clone().take(64) {
            costs[count] = ch.len_utf8();
            count += 1;
        }
        let granted = match admission.charge_prefix(&costs[..count]).await {
            Ok(granted) => granted,
            Err(failure) => {
                drop(copied);
                admission.release(source.len()).await;
                return Err(failure);
            }
        };
        for _ in 0..granted {
            copied.push(source_chars.next().expect("granted source character"));
        }
    }
    Ok(copied)
}

async fn optional_string(
    source: Option<&String>,
    admission: &Admission,
) -> Result<Option<String>, RequestFailure> {
    match source {
        Some(source) => Ok(Some(string(source, admission).await?)),
        None => Ok(None),
    }
}

async fn vector<T>(len: usize, admission: &Admission) -> Result<Vec<T>, RequestFailure> {
    admission
        .reserve(len.saturating_mul(size_of::<T>()))
        .await?;
    Ok(Vec::with_capacity(len))
}

fn hash_bucket_count(len: usize) -> usize {
    if len == 0 {
        0
    } else {
        len.saturating_mul(8)
            .div_ceil(7)
            .max(4)
            .checked_next_power_of_two()
            .unwrap_or(usize::MAX)
    }
}

fn set_storage_bytes<T>(len: usize) -> usize {
    hash_bucket_count(len).saturating_mul(size_of::<T>().saturating_add(1))
}

fn map_storage_bytes<K, V>(len: usize) -> usize {
    hash_bucket_count(len).saturating_mul(size_of::<(K, V)>().saturating_add(1))
}

/// Transfer one private map through admitted record moves when its next
/// insertion would rehash. The old buckets remain live until the new map is
/// complete; a hard stop discards the owning fork and its partial transfer.
async fn grow_map_for<K: Eq + Hash, V>(
    map: &mut HashMap<K, V>,
    needed: usize,
    admission: &Admission,
) -> Result<(), RequestFailure> {
    if map.capacity() >= needed {
        return Ok(());
    }
    let old_bytes = map_storage_bytes::<K, V>(map.capacity());
    let capacity = admission
        .reserve_map_growth(MapGrowthSpec {
            needed,
            old_capacity: map.capacity(),
            bucket_bytes: size_of::<(K, V)>().saturating_add(1),
        })
        .await?;
    let mut replacement = HashMap::with_capacity(capacity);
    let mut old = std::mem::take(map).into_iter();
    while let Some((key, value)) = old.next() {
        admission.checkpoint().await?;
        admission.charge(size_of::<(K, V)>()).await?;
        replacement.insert(key, value);
    }
    drop(old);
    *map = replacement;
    admission.release(old_bytes).await;
    Ok(())
}

async fn grow_vec_for<T>(
    values: &mut Vec<T>,
    needed: usize,
    admission: &Admission,
) -> Result<(), RequestFailure> {
    if values.capacity() >= needed {
        return Ok(());
    }
    debug_assert!(size_of::<T>() > 0);
    let old_bytes = values.capacity().saturating_mul(size_of::<T>());
    let capacity = admission
        .reserve_vec_growth(VecGrowthSpec {
            needed,
            old_capacity: values.capacity(),
            element_bytes: size_of::<T>(),
        })
        .await?;
    let mut replacement = Vec::with_capacity(capacity);
    let mut old = std::mem::take(values).into_iter();
    while let Some(value) = old.next() {
        admission.checkpoint().await?;
        admission.charge(size_of::<T>()).await?;
        replacement.push(value);
    }
    drop(old);
    *values = replacement;
    admission.release(old_bytes).await;
    Ok(())
}

async fn junction_heap_bytes(
    junction: &Junction,
    admission: &Admission,
) -> Result<usize, RequestFailure> {
    let mut bytes = junction.id.len()
        + junction.component.len()
        + junction.connections.capacity() * size_of::<String>();
    for id in &junction.connections {
        admission.checkpoint().await?;
        bytes = bytes.saturating_add(id.len());
    }
    Ok(bytes)
}

async fn bridge_heap_bytes(
    bridge: &Bridge,
    admission: &Admission,
) -> Result<usize, RequestFailure> {
    let mut bytes = bridge.id.len() + bridge.component.len();
    for channel in &bridge.channels {
        bytes = bytes
            .saturating_add(channel.commodity.len())
            .saturating_add(channel.connections.capacity() * size_of::<String>());
        for id in &channel.connections {
            admission.checkpoint().await?;
            bytes = bytes.saturating_add(id.len());
        }
    }
    Ok(bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GrowthOrder {
    VecThenSet,
    SetThenVec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct JointGrowthPlan {
    order: GrowthOrder,
    vec_capacity: usize,
    set_capacity: usize,
    set_buckets: usize,
    escrow_bytes: usize,
}

#[derive(Clone, Copy)]
struct JointGrowthInput {
    needed_cells: usize,
    old_vec_capacity: usize,
    old_set_capacity: usize,
    old_vec_bytes: usize,
    old_set_bytes: usize,
    current_reserved_bytes: usize,
    stable_peak_cap: usize,
}

fn set_capacity_for_buckets(buckets: usize) -> usize {
    if buckets == 4 {
        3
    } else {
        buckets.saturating_sub(buckets / 8)
    }
}

/// Choose both Extend destinations against the maximum simultaneous live
/// reservation, including the old fork containers and prepared cells. Hash
/// bucket steps are few; the vector bound is solved directly for each step.
fn plan_joint_extend_growth(input: JointGrowthInput) -> Option<JointGrowthPlan> {
    let headroom = input
        .stable_peak_cap
        .checked_sub(input.current_reserved_bytes)?;
    let point_bytes = size_of::<Point>();
    let vec_grows = input.old_vec_capacity < input.needed_cells;
    let set_grows = input.old_set_capacity < input.needed_cells;
    let vec_geometric = input
        .old_vec_capacity
        .saturating_mul(2)
        .max(input.needed_cells);
    let set_geometric = input
        .old_set_capacity
        .saturating_mul(2)
        .max(input.needed_cells);
    let set_first_bucket = if set_grows {
        hash_bucket_count(input.needed_cells)
    } else {
        0
    };
    let set_last_bucket = if set_grows {
        hash_bucket_count(set_geometric)
    } else {
        0
    };
    let mut best: Option<JointGrowthPlan> = None;
    let mut buckets = set_first_bucket;
    loop {
        let (set_capacity, set_bytes) = if set_grows {
            (
                set_capacity_for_buckets(buckets),
                buckets.checked_mul(point_bytes.checked_add(1)?)?,
            )
        } else {
            (input.old_set_capacity, 0)
        };
        for order in [GrowthOrder::VecThenSet, GrowthOrder::SetThenVec] {
            let vec_capacity = if vec_grows {
                let available = match (set_grows, order) {
                    (false, _) => headroom,
                    (true, GrowthOrder::VecThenSet) => {
                        let Some(available) =
                            headroom.checked_sub(set_bytes.saturating_sub(input.old_vec_bytes))
                        else {
                            continue;
                        };
                        available
                    }
                    (true, GrowthOrder::SetThenVec) => {
                        if set_bytes > headroom {
                            continue;
                        }
                        let Some(available) = headroom
                            .checked_sub(set_bytes)?
                            .checked_add(input.old_set_bytes)
                        else {
                            continue;
                        };
                        available
                    }
                };
                vec_geometric.min(available / point_bytes)
            } else {
                input.old_vec_capacity
            };
            if vec_capacity < input.needed_cells || set_capacity < input.needed_cells {
                continue;
            }
            let vec_bytes = if vec_grows {
                vec_capacity.checked_mul(point_bytes)?
            } else {
                0
            };
            let escrow_bytes = match (vec_grows, set_grows, order) {
                (true, true, GrowthOrder::VecThenSet) => vec_bytes.max(
                    vec_bytes
                        .saturating_sub(input.old_vec_bytes)
                        .saturating_add(set_bytes),
                ),
                (true, true, GrowthOrder::SetThenVec) => set_bytes.max(
                    set_bytes
                        .saturating_sub(input.old_set_bytes)
                        .saturating_add(vec_bytes),
                ),
                (true, false, _) => vec_bytes,
                (false, true, _) => set_bytes,
                (false, false, _) => 0,
            };
            if escrow_bytes > headroom {
                continue;
            }
            let candidate = JointGrowthPlan {
                order,
                vec_capacity,
                set_capacity,
                set_buckets: buckets,
                escrow_bytes,
            };
            let score = |plan: &JointGrowthPlan| {
                let vec_spare = plan.vec_capacity - input.needed_cells;
                let set_spare = plan.set_capacity - input.needed_cells;
                (
                    vec_spare.min(set_spare),
                    vec_spare.saturating_add(set_spare),
                    std::cmp::Reverse(plan.escrow_bytes),
                )
            };
            if best.is_none_or(|current| score(&candidate) > score(&current)) {
                best = Some(candidate);
            }
        }
        if !set_grows || buckets == set_last_bucket {
            break;
        }
        buckets = buckets.checked_mul(2)?;
        if buckets > set_last_bucket {
            break;
        }
    }
    best
}

async fn map<K: Eq + Hash, V>(
    len: usize,
    admission: &Admission,
) -> Result<HashMap<K, V>, RequestFailure> {
    let bytes = map_storage_bytes::<K, V>(len);
    admission.reserve(bytes).await?;
    Ok(HashMap::with_capacity(len))
}

async fn set<T: Eq + Hash>(
    len: usize,
    admission: &Admission,
) -> Result<HashSet<T>, RequestFailure> {
    let bytes = set_storage_bytes::<T>(len);
    admission.reserve(bytes).await?;
    Ok(HashSet::with_capacity(len))
}

async fn route(source: &Route, admission: &Admission) -> Result<Route, RequestFailure> {
    admission.charge(size_of::<Route>()).await?;
    let connection = string(&source.connection, admission).await?;
    let source_port = optional_string(source.source_port.as_ref(), admission).await?;
    let target_port = optional_string(source.target_port.as_ref(), admission).await?;
    let mut cells = vector::<Point>(source.cells.len(), admission).await?;
    while cells.len() < source.cells.len() {
        let count = (source.cells.len() - cells.len()).min(64);
        let costs = [size_of::<Point>(); 64];
        let granted = admission.charge_prefix(&costs[..count]).await?;
        let start = cells.len();
        cells.extend_from_slice(&source.cells[start..start + granted]);
    }
    Ok(Route {
        connection,
        source_port,
        target_port,
        cells,
    })
}

async fn bridge_channel(
    source: &BridgeChannel,
    admission: &Admission,
) -> Result<BridgeChannel, RequestFailure> {
    bridge_channel_with_extra(source, None, admission).await
}

async fn bridge_channel_with_extra(
    source: &BridgeChannel,
    extra: Option<&str>,
    admission: &Admission,
) -> Result<BridgeChannel, RequestFailure> {
    let commodity = string(&source.commodity, admission).await?;
    let insert = extra.and_then(|id| {
        source
            .connections
            .binary_search_by(|existing| existing.as_str().cmp(id))
            .err()
            .map(|position| (position, id))
    });
    let mut connections = vector::<String>(
        source.connections.len() + usize::from(insert.is_some()),
        admission,
    )
    .await?;
    for (index, id) in source.connections.iter().enumerate() {
        if let Some((_, extra)) = insert.filter(|(position, _)| *position == index) {
            admission.charge(size_of::<String>()).await?;
            connections.push(string(extra, admission).await?);
        }
        admission.charge(size_of::<String>()).await?;
        connections.push(string(id, admission).await?);
    }
    if let Some((_, extra)) = insert.filter(|(position, _)| *position == source.connections.len()) {
        admission.charge(size_of::<String>()).await?;
        connections.push(string(extra, admission).await?);
    }
    Ok(BridgeChannel {
        axis: source.axis,
        commodity,
        connections,
    })
}

async fn bridge(source: &Bridge, admission: &Admission) -> Result<Bridge, RequestFailure> {
    admission.charge(size_of::<Bridge>()).await?;
    Ok(Bridge {
        id: string(&source.id, admission).await?,
        component: string(&source.component, admission).await?,
        position: source.position,
        rotation: source.rotation,
        channels: [
            bridge_channel(&source.channels[0], admission).await?,
            bridge_channel(&source.channels[1], admission).await?,
        ],
    })
}

async fn junction(source: &Junction, admission: &Admission) -> Result<Junction, RequestFailure> {
    admission.charge(size_of::<Junction>()).await?;
    let mut connections = vector::<String>(source.connections.len(), admission).await?;
    for id in &source.connections {
        admission.charge(size_of::<String>()).await?;
        connections.push(string(id, admission).await?);
    }
    Ok(Junction {
        id: string(&source.id, admission).await?,
        component: string(&source.component, admission).await?,
        position: source.position,
        rotation: source.rotation,
        connections,
    })
}

async fn active_route(
    source: &ActiveRoute,
    admission: &Admission,
) -> Result<ActiveRoute, RequestFailure> {
    admission.charge(size_of::<ActiveRoute>()).await?;
    let mut cells = vector::<Point>(source.cells.len(), admission).await?;
    while cells.len() < source.cells.len() {
        let count = (source.cells.len() - cells.len()).min(64);
        let costs = [size_of::<Point>(); 64];
        let granted = admission.charge_prefix(&costs[..count]).await?;
        let start = cells.len();
        cells.extend_from_slice(&source.cells[start..start + granted]);
    }
    let mut bridge_reservations =
        vector::<BridgeReservation>(source.bridge_reservations.len(), admission).await?;
    for reserved in &source.bridge_reservations {
        admission.charge(size_of::<BridgeReservation>()).await?;
        bridge_reservations.push(BridgeReservation {
            position: reserved.position,
            component: string(&reserved.component, admission).await?,
            rotation: reserved.rotation,
            crossed_axis: reserved.crossed_axis,
            direction: reserved.direction,
        });
    }
    Ok(ActiveRoute {
        connection: source.connection,
        source_port: optional_string(source.source_port.as_ref(), admission).await?,
        target_port: optional_string(source.target_port.as_ref(), admission).await?,
        target: source.target,
        cells,
        bridge_reservations,
    })
}

async fn copy_session(source: &Session, admission: Admission) -> Result<Session, RequestFailure> {
    admission.reserve(size_of::<Session>()).await?;
    let mut fork = Session {
        instance_id: source.instance_id.clone(),
        problem: source.problem.clone(),
        catalogs: source.catalogs.clone(),
        limits: source.limits,
        blocks: source.blocks.clone(),
        terminals: source.terminals.clone(),
        connections: source.connections.clone(),
        poses: Vec::new(),
        terminal_poses: Vec::new(),
        terminal_at: HashMap::new(),
        routes: Vec::new(),
        junctions: HashMap::new(),
        bridges: HashMap::new(),
        active: None,
        active_cells: HashSet::new(),
        focus: source.focus.clone(),
        bodies: HashSet::new(),
        occupied: std::array::from_fn(|_| HashMap::new()),
        used_ports: HashMap::new(),
        turns: source.turns,
        revision: source.revision,
        metrics: source.metrics.clone(),
        final_failure: None,
        replay_deadline: source.replay_deadline,
        replay_meter: source.replay_meter.clone(),
    };

    fork.poses = vector::<Option<Pose>>(source.poses.len(), &admission).await?;
    for pose in &source.poses {
        admission.charge(size_of::<Option<Pose>>()).await?;
        fork.poses.push(match pose {
            Some(pose) => Some(Pose {
                block: string(&pose.block, &admission).await?,
                position: pose.position,
                rotation: pose.rotation,
            }),
            None => None,
        });
    }
    fork.terminal_poses =
        vector::<Option<TerminalPose>>(source.terminal_poses.len(), &admission).await?;
    for pose in &source.terminal_poses {
        admission.charge(size_of::<Option<TerminalPose>>()).await?;
        fork.terminal_poses.push(match pose {
            Some(pose) => Some(TerminalPose {
                terminal: string(&pose.terminal, &admission).await?,
                position: pose.position,
            }),
            None => None,
        });
    }
    fork.routes = vector::<Option<Route>>(source.routes.len(), &admission).await?;
    for route_value in &source.routes {
        fork.routes.push(match route_value {
            Some(route_value) => Some(route(route_value, &admission).await?),
            None => {
                admission.charge(size_of::<Option<Route>>()).await?;
                None
            }
        });
    }

    fork.terminal_at = map(source.terminal_at.len(), &admission).await?;
    for (&point, &index) in &source.terminal_at {
        admission.charge(size_of::<(Point, usize)>()).await?;
        fork.terminal_at.insert(point, index);
    }
    fork.active_cells = set(source.active_cells.len(), &admission).await?;
    for &point in &source.active_cells {
        admission.charge(size_of::<Point>()).await?;
        fork.active_cells.insert(point);
    }
    fork.bodies = set(source.bodies.len(), &admission).await?;
    for &point in &source.bodies {
        admission.charge(size_of::<Point>()).await?;
        fork.bodies.insert(point);
    }
    for layer in 0..2 {
        fork.occupied[layer] = map(source.occupied[layer].len(), &admission).await?;
        let mut source_entries = source.occupied[layer].iter();
        while source_entries.len() > 0 {
            let mut costs = [0; 64];
            let mut count = 0;
            let mut user_bytes = 0usize;
            for (_, users) in source_entries.clone().take(64) {
                if users.len() > 4 {
                    break;
                }
                let vector_bytes = users.len().saturating_mul(size_of::<usize>());
                if count > 0 && user_bytes.saturating_add(vector_bytes) > 4_096 {
                    break;
                }
                costs[count] = size_of::<(Point, Vec<usize>)>().saturating_add(vector_bytes);
                user_bytes = user_bytes.saturating_add(vector_bytes);
                count += 1;
            }
            if count == 0 {
                let (&point, users) = source_entries.next().expect("occupied entry exists");
                let mut copied_users = vector::<usize>(users.len(), &admission).await?;
                admission.charge(size_of::<(Point, Vec<usize>)>()).await?;
                while copied_users.len() < users.len() {
                    let chunk_len = (users.len() - copied_users.len()).min(64);
                    let costs = [size_of::<usize>(); 64];
                    let granted = admission.charge_prefix(&costs[..chunk_len]).await?;
                    let start = copied_users.len();
                    copied_users.extend_from_slice(&users[start..start + granted]);
                }
                fork.occupied[layer].insert(point, copied_users);
                continue;
            }
            admission.reserve(user_bytes).await?;
            let mut copied = 0;
            while copied < count {
                let granted = admission.charge_prefix(&costs[copied..count]).await?;
                for _ in 0..granted {
                    let (&point, users) = source_entries.next().expect("granted source entry");
                    fork.occupied[layer].insert(point, users.clone());
                }
                copied += granted;
            }
        }
    }
    fork.junctions = map(source.junctions.len(), &admission).await?;
    for (&key, value) in &source.junctions {
        fork.junctions
            .insert(key, junction(value, &admission).await?);
    }
    fork.bridges = map(source.bridges.len(), &admission).await?;
    for (&key, value) in &source.bridges {
        fork.bridges.insert(key, bridge(value, &admission).await?);
    }
    fork.used_ports = map(source.used_ports.len(), &admission).await?;
    for ((index, id), used) in &source.used_ports {
        admission
            .charge(size_of::<((usize, String), UsedPort)>())
            .await?;
        fork.used_ports.insert(
            (*index, string(id, &admission).await?),
            UsedPort {
                commodity: string(&used.commodity, &admission).await?,
                transport: used.transport,
                direction: used.direction,
                rate: used.rate,
            },
        );
    }
    fork.active = match &source.active {
        Some(active) => Some(active_route(active, &admission).await?),
        None => None,
    };
    fork.final_failure = match &source.final_failure {
        Some(failure) => Some(Diagnostic {
            code: string(&failure.code, &admission).await?,
            severity: failure.severity,
            stage: failure.stage,
            message: string(&failure.message, &admission).await?,
            entity: string(&failure.entity, &admission).await?,
        }),
        None => None,
    };
    Ok(fork)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructive::routing_candidates::{
        AllowanceCaps, CUMULATIVE_WORK_KINDS, SearchMemoryBudget,
    };
    use std::time::Duration;

    #[test]
    fn map_growth_moves_entries_across_pages_and_stops_before_impossible_peak() {
        let mut source = HashMap::with_capacity(3);
        for x in 0..3 {
            source.insert(Point { x, y: 1 }, vec![x as usize]);
        }
        let needed = source.len() + 1;
        let old_bytes = map_storage_bytes::<Point, Vec<usize>>(source.capacity());
        let new_bytes = map_storage_bytes::<Point, Vec<usize>>(needed);
        let expected = source.clone();
        let mut caps = AllowanceCaps::default();
        caps.peak_live_bytes = old_bytes + new_bytes;
        let admission = Admission::default();
        admission.0.borrow_mut().reserved_bytes = old_bytes;
        let mut allowance = RequestAllowance::new(caps.clone());
        allowance.reserve_peak(old_bytes).unwrap();
        let mut future: Option<Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>> =
            Some(Box::pin(grow_map_for(&mut source, needed, &admission)));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        quotas[CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap()] = 16;
        let mut pages = 0;
        loop {
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = poll_admitted(&mut future, &admission, &mut allowance, 0);
            allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    pages += 1;
                }
                AdmittedAdvance::Complete(Ok(())) => break,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("map growth failed: {failure:?}")
                }
            }
        }
        assert!(pages > 0);
        drop(future);
        assert_eq!(source, expected);
        assert!(source.capacity() >= needed);
        assert_eq!(admission.0.borrow().reserved_bytes, new_bytes);
        assert_eq!(allowance.reserved_bytes(), new_bytes);
        allowance.release_peak(new_bytes);

        caps.peak_live_bytes -= 1;
        let mut denied = expected.clone();
        let denied_admission = Admission::default();
        denied_admission.0.borrow_mut().reserved_bytes = old_bytes;
        let mut denied_allowance = RequestAllowance::new(caps);
        denied_allowance.reserve_peak(old_bytes).unwrap();
        let mut denied_future: Option<
            Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>,
        > = Some(Box::pin(grow_map_for(
            &mut denied,
            needed,
            &denied_admission,
        )));
        assert!(matches!(
            poll_admitted(
                &mut denied_future,
                &denied_admission,
                &mut denied_allowance,
                0
            ),
            AdmittedAdvance::Complete(Err(RequestFailure::Stopped(_)))
        ));
        drop(denied_future);
        assert_eq!(denied, expected);
        assert_eq!(denied_admission.0.borrow().reserved_bytes, old_bytes);
        denied_allowance.release_peak(old_bytes);
    }

    #[test]
    fn vector_growth_moves_old_users_across_pages() {
        let mut users = Vec::with_capacity(2);
        users.extend([4usize, 7]);
        let old_bytes = users.capacity() * size_of::<usize>();
        let new_bytes = 4 * size_of::<usize>();
        let mut caps = AllowanceCaps::default();
        caps.peak_live_bytes = old_bytes + new_bytes;
        let admission = Admission::default();
        admission.0.borrow_mut().reserved_bytes = old_bytes;
        let mut allowance = RequestAllowance::new(caps);
        allowance.reserve_peak(old_bytes).unwrap();
        let mut future: Option<Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>> =
            Some(Box::pin(grow_vec_for(&mut users, 3, &admission)));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        quotas[CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap()] = 4;
        let mut pages = 0;
        loop {
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = poll_admitted(&mut future, &admission, &mut allowance, 0);
            allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    pages += 1;
                }
                AdmittedAdvance::Complete(Ok(())) => break,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("vector growth failed: {failure:?}")
                }
            }
        }
        drop(future);
        assert!(pages > 0);
        assert_eq!(users, [4, 7]);
        assert_eq!(users.capacity(), 4);
        assert_eq!(admission.0.borrow().reserved_bytes, new_bytes);
        assert_eq!(allowance.reserved_bytes(), new_bytes);
        allowance.release_peak(new_bytes);
    }

    #[test]
    fn admitted_junction_options_resume_catalog_visits_with_native_result() {
        let catalogs = crate::constructive::tests::catalogs();
        let rate = Rate {
            numerator: 1,
            denominator: 4,
        };
        let uses = [
            SiteArmUse {
                connection: "west-to-north",
                commodity: "ore",
                rate,
                incoming: Some(3),
                outgoing: Some(0),
                external_incoming: false,
                external_outgoing: false,
            },
            SiteArmUse {
                connection: "west-to-east",
                commodity: "ore",
                rate,
                incoming: Some(3),
                outgoing: Some(1),
                external_incoming: false,
                external_outgoing: false,
            },
        ];
        let point = Point { x: 10, y: 20 };
        let expected = aic_layout::validation::junction_options_with_visit(
            aic_layout::logistics::TransportKind::Belt,
            point,
            &uses,
            &catalogs,
            || true,
        )
        .unwrap();
        assert!(expected.is_some());
        let admission = Admission::default();
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut future: Option<
            Pin<
                Box<
                    dyn Future<Output = Result<(Option<Vec<(String, i64)>>, usize), RequestFailure>>
                        + '_,
                >,
            >,
        > = Some(Box::pin(junction_options_admitted(
            aic_layout::logistics::TransportKind::Belt,
            point,
            &uses,
            &catalogs,
            &admission,
        )));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        quotas[CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CatalogVisits)
            .unwrap()] = 1;
        let mut pages = 0;
        let (actual, bytes) = loop {
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = poll_admitted(&mut future, &admission, &mut allowance, 0);
            allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    pages += 1;
                }
                AdmittedAdvance::Complete(Ok(result)) => break result,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("staged junction failed: {failure:?}")
                }
            }
        };
        assert!(pages > 0);
        assert_eq!(actual, expected);
        assert_eq!(bytes, admission.0.borrow().reserved_bytes);
        let paged_work = allowance.usage().categories;
        drop(actual);
        allowance.release_peak(bytes);

        let unpaged_admission = Admission::default();
        let mut unpaged_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut unpaged_future: Option<
            Pin<
                Box<
                    dyn Future<Output = Result<(Option<Vec<(String, i64)>>, usize), RequestFailure>>
                        + '_,
                >,
            >,
        > = Some(Box::pin(junction_options_admitted(
            aic_layout::logistics::TransportKind::Belt,
            point,
            &uses,
            &catalogs,
            &unpaged_admission,
        )));
        let AdmittedAdvance::Complete(Ok((unpaged, unpaged_bytes))) = poll_admitted(
            &mut unpaged_future,
            &unpaged_admission,
            &mut unpaged_allowance,
            0,
        ) else {
            panic!("unpaged junction must complete");
        };
        assert_eq!(unpaged, expected);
        assert_eq!(paged_work, unpaged_allowance.usage().categories);
        drop(unpaged);
        unpaged_allowance.release_peak(unpaged_bytes);

        let mut long_catalogs = Arc::try_unwrap(crate::constructive::tests::catalogs())
            .ok()
            .expect("fresh catalog has one owner");
        let mut raw = long_catalogs.components.catalog().clone();
        let long_id = format!("log-splitter-{}", "a".repeat(8192));
        raw.components
            .iter_mut()
            .find(|definition| definition.id == "log-splitter")
            .unwrap()
            .id = long_id.clone();
        long_catalogs.components =
            aic_layout::logistics::ValidatedLogisticsComponentCatalog::try_from_catalog(raw)
                .unwrap();
        let long_expected = aic_layout::validation::junction_options_with_visit(
            aic_layout::logistics::TransportKind::Belt,
            point,
            &uses,
            &long_catalogs,
            || true,
        )
        .unwrap();
        assert_eq!(long_expected.as_ref().unwrap()[0].0, long_id);
        let long_admission = Admission::default();
        let mut long_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut long_future: Option<
            Pin<
                Box<
                    dyn Future<Output = Result<(Option<Vec<(String, i64)>>, usize), RequestFailure>>
                        + '_,
                >,
            >,
        > = Some(Box::pin(junction_options_admitted(
            aic_layout::logistics::TransportKind::Belt,
            point,
            &uses,
            &long_catalogs,
            &long_admission,
        )));
        let mut copy_quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        copy_quotas[CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap()] = 1024;
        let mut copy_pages = 0;
        let (long_actual, long_bytes) = loop {
            long_allowance.begin_page_window(copy_quotas, Duration::from_secs(10));
            let result = poll_admitted(&mut long_future, &long_admission, &mut long_allowance, 0);
            long_allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    copy_pages += 1;
                }
                AdmittedAdvance::Complete(Ok(result)) => break result,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("long option copy failed: {failure:?}")
                }
            }
        };
        assert!(copy_pages >= 7);
        assert_eq!(long_actual, long_expected);
        assert_eq!(
            long_allowance.usage().categories[&WorkKind::CopiedBytes],
            long_id.len()
        );
        drop(long_actual);
        long_allowance.release_peak(long_bytes);

        let mut hard_caps = AllowanceCaps::default();
        hard_caps.copied_bytes = 2048;
        let hard_admission = Admission::default();
        let mut hard_allowance = RequestAllowance::new(hard_caps);
        let mut hard_future: Option<
            Pin<
                Box<
                    dyn Future<Output = Result<(Option<Vec<(String, i64)>>, usize), RequestFailure>>
                        + '_,
                >,
            >,
        > = Some(Box::pin(junction_options_admitted(
            aic_layout::logistics::TransportKind::Belt,
            point,
            &uses,
            &long_catalogs,
            &hard_admission,
        )));
        assert!(matches!(
            poll_admitted(&mut hard_future, &hard_admission, &mut hard_allowance, 0),
            AdmittedAdvance::Complete(Err(RequestFailure::Stopped(_)))
        ));
        assert_eq!(hard_admission.0.borrow().reserved_bytes, 0);
        assert_eq!(hard_allowance.reserved_bytes(), 0);
    }

    #[test]
    fn admitted_junction_updates_create_shared_splitter() {
        use super::super::{Command, Limits};
        use aic_layout::logistics::TransportKind;
        use aic_layout::physical::{Block, Connection, Problem, Terminal, TerminalDirection};

        let problem = Problem {
            blocks: vec![Block {
                id: "machine".into(),
                facility: "grinder-1-mode-normal".into(),
            }],
            terminals: [
                ("supply", TerminalDirection::Input),
                ("sink-a", TerminalDirection::Output),
                ("sink-b", TerminalDirection::Output),
            ]
            .into_iter()
            .map(|(id, direction)| Terminal {
                id: id.into(),
                direction,
                commodity: "test-item".into(),
                transport: TransportKind::Belt,
            })
            .collect(),
            connections: [
                ("supply-flow", "supply", "machine", 2),
                ("output-a", "machine", "sink-a", 4),
                ("output-b", "machine", "sink-b", 4),
            ]
            .into_iter()
            .map(|(id, source, target, denominator)| Connection {
                id: id.into(),
                source: source.into(),
                target: target.into(),
                commodity: "test-item".into(),
                transport: TransportKind::Belt,
                rate: Rate {
                    numerator: 1,
                    denominator,
                },
            })
            .collect(),
        };
        let mut session = Session::new(
            problem,
            crate::constructive::tests::catalogs(),
            Limits {
                max_turns: 32,
                max_extension: 16,
            },
        )
        .unwrap();
        for command in [
            Command::Place {
                block: "machine".into(),
                position: Point { x: 5, y: 8 },
                rotation: 0,
            },
            Command::PlaceTerminal {
                terminal: "supply".into(),
                position: Point { x: 6, y: 15 },
            },
            Command::PlaceTerminal {
                terminal: "sink-a".into(),
                position: Point { x: 4, y: 0 },
            },
            Command::PlaceTerminal {
                terminal: "sink-b".into(),
                position: Point { x: 8, y: 0 },
            },
            Command::BeginRoute {
                connection: "supply-flow".into(),
                source_port: None,
                target_port: Some("input-belt-1".into()),
            },
            Command::Extend {
                direction: Direction::North,
                length: 4,
            },
            Command::Connect,
            Command::BeginRoute {
                connection: "output-a".into(),
                source_port: Some("output-belt-1".into()),
                target_port: None,
            },
            Command::Extend {
                direction: Direction::North,
                length: 3,
            },
            Command::Extend {
                direction: Direction::West,
                length: 2,
            },
            Command::Extend {
                direction: Direction::North,
                length: 4,
            },
            Command::Connect,
            Command::BeginRoute {
                connection: "output-b".into(),
                source_port: Some("output-belt-1".into()),
                target_port: None,
            },
            Command::Extend {
                direction: Direction::North,
                length: 3,
            },
            Command::Extend {
                direction: Direction::East,
                length: 2,
            },
            Command::Extend {
                direction: Direction::North,
                length: 4,
            },
        ] {
            let step = session.step(command);
            assert!(step.accepted, "{:?}", step.diagnostic);
        }
        let active = session.active.as_ref().unwrap();
        assert_eq!(
            session.active_cells,
            active.cells.iter().copied().collect::<HashSet<_>>()
        );
        let view = RouteView::from(active);
        let expected = session.junction_updates(2, view).unwrap();
        assert!(expected.iter().any(|(_, junction)| junction.is_some()));
        let admission = Admission::default();
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut future: Option<
            Pin<Box<dyn Future<Output = Result<PreparedJunctionUpdates, RequestFailure>> + '_>>,
        > = Some(Box::pin(prepare_junction_updates(
            &session, 2, view, &admission,
        )));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        for (kind, quota) in [
            (WorkKind::ReplayRouteCells, 1),
            (WorkKind::ReplayUsers, 1),
            (WorkKind::CatalogVisits, 1),
            (WorkKind::CopiedBytes, 64),
        ] {
            quotas[CUMULATIVE_WORK_KINDS
                .iter()
                .position(|candidate| *candidate == kind)
                .unwrap()] = quota;
        }
        let mut pages = 0;
        let actual = loop {
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = poll_admitted(&mut future, &admission, &mut allowance, 0);
            allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    pages += 1;
                }
                AdmittedAdvance::Complete(Ok(updates)) => break updates,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("staged splitter junction failed: {failure:?}")
                }
            }
        };
        assert!(pages > 0);
        assert_eq!(actual.updates, expected);
        let new_occupied_sites = actual.new_occupied_sites;
        drop(actual);
        allowance.release_peak(admission.0.borrow().reserved_bytes);

        let connect_admission = Admission::default();
        let mut connect_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut connect_future: Option<
            Pin<Box<dyn Future<Output = Result<PreparedConnect, RequestFailure>> + '_>>,
        > = Some(Box::pin(prepare_connect(&session, &connect_admission)));
        let AdmittedAdvance::Complete(Ok(mut prepared)) = poll_admitted(
            &mut connect_future,
            &connect_admission,
            &mut connect_allowance,
            0,
        ) else {
            panic!("prepared Connect must complete");
        };
        assert_eq!(prepared.connection_index, 2);
        assert_eq!(prepared.route_connection, "output-b");
        assert_eq!(prepared.new_occupied_sites, new_occupied_sites);
        assert_eq!(prepared.junctions, expected);
        assert!(prepared.bridges.is_empty());
        let mut ordinary = session.clone();
        assert!(ordinary.step(Command::Connect).accepted);
        let mut fork = session.clone();
        let transport_layer = layer(TransportKind::Belt);
        let original_occupied_bytes =
            map_storage_bytes::<Point, Vec<usize>>(fork.occupied[transport_layer].capacity())
                + fork.occupied[transport_layer]
                    .values()
                    .map(|users| users.capacity() * size_of::<usize>())
                    .sum::<usize>();
        connect_allowance
            .reserve_peak(original_occupied_bytes)
            .unwrap();
        connect_admission.0.borrow_mut().reserved_bytes += original_occupied_bytes;
        let mut occupied_future: Option<
            Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>,
        > = Some(Box::pin(commit_connect_occupied(
            &mut fork,
            &prepared,
            &connect_admission,
        )));
        let mut occupied_quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        for (kind, quota) in [(WorkKind::ReplayRouteCells, 1), (WorkKind::CopiedBytes, 16)] {
            occupied_quotas[CUMULATIVE_WORK_KINDS
                .iter()
                .position(|candidate| *candidate == kind)
                .unwrap()] = quota;
        }
        let mut occupied_pages = 0;
        loop {
            connect_allowance.begin_page_window(occupied_quotas, Duration::from_secs(10));
            let result = poll_admitted(
                &mut occupied_future,
                &connect_admission,
                &mut connect_allowance,
                0,
            );
            connect_allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    occupied_pages += 1;
                }
                AdmittedAdvance::Complete(Ok(())) => break,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("private occupancy commit failed: {failure:?}")
                }
            }
        }
        drop(occupied_future);
        assert!(occupied_pages > 0);
        assert_eq!(fork.occupied, ordinary.occupied);
        let original_components_bytes =
            map_storage_bytes::<(TransportKind, Point), Junction>(fork.junctions.capacity())
                + fork
                    .junctions
                    .values()
                    .map(|junction| {
                        junction.id.len()
                            + junction.component.len()
                            + junction.connections.capacity() * size_of::<String>()
                            + junction.connections.iter().map(String::len).sum::<usize>()
                    })
                    .sum::<usize>()
                + map_storage_bytes::<(TransportKind, Point), Bridge>(fork.bridges.capacity())
                + fork
                    .bridges
                    .values()
                    .map(|bridge| {
                        bridge.id.len()
                            + bridge.component.len()
                            + bridge
                                .channels
                                .iter()
                                .map(|channel| {
                                    channel.commodity.len()
                                        + channel.connections.capacity() * size_of::<String>()
                                        + channel.connections.iter().map(String::len).sum::<usize>()
                                })
                                .sum::<usize>()
                    })
                    .sum::<usize>();
        connect_allowance
            .reserve_peak(original_components_bytes)
            .unwrap();
        connect_admission.0.borrow_mut().reserved_bytes += original_components_bytes;
        let mut component_future: Option<
            Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>,
        > = Some(Box::pin(commit_connect_components(
            &mut fork,
            &mut prepared,
            &connect_admission,
        )));
        let mut component_pages = 0;
        loop {
            connect_allowance.begin_page_window(occupied_quotas, Duration::from_secs(10));
            let result = poll_admitted(
                &mut component_future,
                &connect_admission,
                &mut connect_allowance,
                0,
            );
            connect_allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    component_pages += 1;
                }
                AdmittedAdvance::Complete(Ok(())) => break,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("private component commit failed: {failure:?}")
                }
            }
        }
        drop(component_future);
        assert!(component_pages > 0);
        assert_eq!(fork.junctions, ordinary.junctions);
        assert_eq!(fork.bridges, ordinary.bridges);
        let mut final_future: Option<
            Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>,
        > = Some(Box::pin(commit_connect_ports_and_route(
            &mut fork,
            &mut prepared,
            &connect_admission,
        )));
        let mut final_quotas = occupied_quotas;
        final_quotas[CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap()] = 4;
        let mut final_pages = 0;
        loop {
            connect_allowance.begin_page_window(final_quotas, Duration::from_secs(10));
            let result = poll_admitted(
                &mut final_future,
                &connect_admission,
                &mut connect_allowance,
                0,
            );
            connect_allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    final_pages += 1;
                }
                AdmittedAdvance::Complete(Ok(())) => break,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("private Connect final commit failed: {failure:?}")
                }
            }
        }
        drop(final_future);
        assert!(final_pages > 0);
        assert_eq!(fork.used_ports.len(), ordinary.used_ports.len());
        for (key, expected) in &ordinary.used_ports {
            let actual = &fork.used_ports[key];
            assert_eq!(actual.commodity, expected.commodity);
            assert_eq!(actual.transport, expected.transport);
            assert_eq!(actual.direction, expected.direction);
            assert_eq!(actual.rate, expected.rate);
        }
        assert_eq!(fork.routes, ordinary.routes);
        assert!(fork.active.is_none());
        assert_eq!(fork.active_cells, ordinary.active_cells);
        assert_eq!(fork.focus, ordinary.focus);
        drop(prepared);
        connect_allowance.release_peak(connect_admission.0.borrow().reserved_bytes);

        let mut integrated_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut fork_cursor = SessionForkCursor::new(&session);
        let (mut integrated, copied_bytes) = loop {
            integrated_allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = fork_cursor.advance(&mut integrated_allowance, 0);
            integrated_allowance.end_page_window();
            match result {
                ForkAdvance::Pending(stop) => assert_eq!(stop.reason, "page-quantum"),
                ForkAdvance::Complete(_, Ok(fork)) => break fork,
                ForkAdvance::Complete(_, Err(failure)) => {
                    panic!("private source fork failed: {failure:?}")
                }
            }
        };
        let integrated_admission = Admission::default();
        integrated_admission.0.borrow_mut().reserved_bytes = copied_bytes;
        let route_commands = [Command::Connect];
        let mut integrated_future: Option<
            Pin<
                Box<
                    dyn Future<Output = (Vec<ReplayCommandOutcome>, Result<(), RequestFailure>)>
                        + '_,
                >,
            >,
        > = Some(Box::pin(replay_route_commands(
            &mut integrated,
            &route_commands,
            &integrated_admission,
        )));
        let mut integrated_pages = 0;
        let outcomes = loop {
            integrated_allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = poll_admitted(
                &mut integrated_future,
                &integrated_admission,
                &mut integrated_allowance,
                0,
            );
            integrated_allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    integrated_pages += 1;
                }
                AdmittedAdvance::Complete((outcomes, Ok(()))) => break outcomes,
                AdmittedAdvance::Complete((_, Err(failure))) => {
                    panic!("private Connect failed: {failure:?}")
                }
            }
        };
        drop(integrated_future);
        assert!(integrated_pages > 0);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].turn, ordinary.turns);
        assert_eq!(outcomes[0].revision, ordinary.revision);
        assert_eq!(outcomes[0].progress_before, session.construction_progress());
        assert_eq!(outcomes[0].progress_after, ordinary.construction_progress());
        assert!(outcomes[0].terminal);
        assert_eq!(integrated.turns, ordinary.turns);
        assert_eq!(integrated.revision, ordinary.revision);
        assert_eq!(integrated.occupied, ordinary.occupied);
        assert_eq!(integrated.junctions, ordinary.junctions);
        assert_eq!(integrated.bridges, ordinary.bridges);
        assert_eq!(integrated.routes, ordinary.routes);
        assert!(integrated.active.is_none());
        assert_eq!(integrated.active_cells, ordinary.active_cells);
        assert_eq!(integrated.focus, ordinary.focus);
        drop(integrated);
        integrated_allowance.release_peak(integrated_admission.0.borrow().reserved_bytes);
    }

    #[test]
    fn admitted_site_arms_resume_selected_port_visits_without_route_copy() {
        use super::super::Command;

        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        assert!(
            source
                .step(Command::BeginRoute {
                    connection: "connection-0".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        let active = source.active.as_ref().unwrap();
        let length = active.cells[0].y - active.target.y;
        assert!(
            source
                .step(Command::Extend {
                    direction: Direction::North,
                    length,
                })
                .accepted
        );
        let view = RouteView::from(source.active.as_ref().unwrap());
        for path_index in [0, view.cells.len() - 1] {
            let expected = source.site_arm_use_view(0, view, path_index).unwrap();
            let admission = Admission::default();
            let mut allowance = RequestAllowance::new(AllowanceCaps::default());
            let mut future: Option<
                Pin<Box<dyn Future<Output = Result<SiteArmUse<'_>, RequestFailure>> + '_>>,
            > = Some(Box::pin(site_arm_use_admitted(
                &source, 0, view, path_index, &admission,
            )));
            let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
            quotas[CUMULATIVE_WORK_KINDS
                .iter()
                .position(|kind| *kind == WorkKind::CatalogVisits)
                .unwrap()] = 1;
            let mut pages = 0;
            let actual = loop {
                allowance.begin_page_window(quotas, Duration::from_secs(10));
                let result = poll_admitted(&mut future, &admission, &mut allowance, 0);
                allowance.end_page_window();
                match result {
                    AdmittedAdvance::Pending(stop) => {
                        assert_eq!(stop.reason, "page-quantum");
                        pages += 1;
                    }
                    AdmittedAdvance::Complete(Ok(arm)) => break arm,
                    AdmittedAdvance::Complete(Err(failure)) => {
                        panic!("valid site arm failed: {failure:?}")
                    }
                }
            };
            assert!(pages > 0);
            assert_eq!(actual.connection, expected.connection);
            assert_eq!(actual.commodity, expected.commodity);
            assert_eq!(actual.rate, expected.rate);
            assert_eq!(actual.incoming, expected.incoming);
            assert_eq!(actual.outgoing, expected.outgoing);
            assert_eq!(actual.external_incoming, expected.external_incoming);
            assert_eq!(actual.external_outgoing, expected.external_outgoing);
            assert_eq!(admission.0.borrow().reserved_bytes, 0);

            let unpaged_admission = Admission::default();
            let mut unpaged_allowance = RequestAllowance::new(AllowanceCaps::default());
            let mut unpaged_future: Option<
                Pin<Box<dyn Future<Output = Result<SiteArmUse<'_>, RequestFailure>> + '_>>,
            > = Some(Box::pin(site_arm_use_admitted(
                &source,
                0,
                view,
                path_index,
                &unpaged_admission,
            )));
            let AdmittedAdvance::Complete(Ok(_)) = poll_admitted(
                &mut unpaged_future,
                &unpaged_admission,
                &mut unpaged_allowance,
                0,
            ) else {
                panic!("unpaged site arm must complete");
            };
            assert_eq!(
                allowance.usage().categories,
                unpaged_allowance.usage().categories
            );
            let selected = if path_index == 0 {
                "output-belt-1"
            } else {
                "input-belt-1"
            };
            let definition = source
                .catalogs
                .facilities
                .facility("grinder-1-mode-normal")
                .unwrap();
            let expected_visits = definition
                .ports
                .iter()
                .position(|port| port.id == selected)
                .unwrap()
                + 1;
            assert_eq!(
                allowance.usage().categories[&WorkKind::CatalogVisits],
                expected_visits
            );
        }
    }

    #[test]
    fn admitted_site_arm_preserves_external_terminal_arm() {
        use super::super::{Command, Limits};
        use aic_layout::logistics::TransportKind;
        use aic_layout::physical::{Block, Connection, Problem, Terminal, TerminalDirection};

        let mut source = Session::new(
            Problem {
                blocks: vec![Block {
                    id: "machine".into(),
                    facility: "grinder-1-mode-normal".into(),
                }],
                terminals: vec![Terminal {
                    id: "supply".into(),
                    direction: TerminalDirection::Input,
                    commodity: "test-item".into(),
                    transport: TransportKind::Belt,
                }],
                connections: vec![Connection {
                    id: "supply-flow".into(),
                    source: "supply".into(),
                    target: "machine".into(),
                    commodity: "test-item".into(),
                    transport: TransportKind::Belt,
                    rate: Rate {
                        numerator: 1,
                        denominator: 2,
                    },
                }],
            },
            crate::constructive::tests::catalogs(),
            Limits {
                max_turns: 8,
                max_extension: 16,
            },
        )
        .unwrap();
        for command in [
            Command::Place {
                block: "machine".into(),
                position: Point { x: 5, y: 8 },
                rotation: 0,
            },
            Command::PlaceTerminal {
                terminal: "supply".into(),
                position: Point { x: 6, y: 15 },
            },
            Command::BeginRoute {
                connection: "supply-flow".into(),
                source_port: None,
                target_port: Some("input-belt-1".into()),
            },
            Command::Extend {
                direction: Direction::North,
                length: 4,
            },
        ] {
            assert!(source.step(command).accepted);
        }
        let view = RouteView::from(source.active.as_ref().unwrap());
        let expected = source.site_arm_use_view(0, view, 0).unwrap();
        assert!(expected.external_incoming);
        let admission = Admission::default();
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut future: Option<
            Pin<Box<dyn Future<Output = Result<SiteArmUse<'_>, RequestFailure>> + '_>>,
        > = Some(Box::pin(site_arm_use_admitted(
            &source, 0, view, 0, &admission,
        )));
        let AdmittedAdvance::Complete(Ok(actual)) =
            poll_admitted(&mut future, &admission, &mut allowance, 0)
        else {
            panic!("external terminal arm must complete");
        };
        assert_eq!(actual.incoming, expected.incoming);
        assert_eq!(actual.outgoing, expected.outgoing);
        assert!(actual.external_incoming);
        assert!(!actual.external_outgoing);
        assert_eq!(
            allowance
                .usage()
                .categories
                .get(&WorkKind::CatalogVisits)
                .copied()
                .unwrap_or(0),
            0
        );

        let mut outgoing = Session::new(
            Problem {
                blocks: vec![Block {
                    id: "machine".into(),
                    facility: "grinder-1-mode-normal".into(),
                }],
                terminals: vec![Terminal {
                    id: "sink".into(),
                    direction: TerminalDirection::Output,
                    commodity: "test-item".into(),
                    transport: TransportKind::Belt,
                }],
                connections: vec![Connection {
                    id: "output-flow".into(),
                    source: "machine".into(),
                    target: "sink".into(),
                    commodity: "test-item".into(),
                    transport: TransportKind::Belt,
                    rate: Rate {
                        numerator: 1,
                        denominator: 2,
                    },
                }],
            },
            crate::constructive::tests::catalogs(),
            Limits {
                max_turns: 8,
                max_extension: 16,
            },
        )
        .unwrap();
        for command in [
            Command::Place {
                block: "machine".into(),
                position: Point { x: 5, y: 8 },
                rotation: 0,
            },
            Command::PlaceTerminal {
                terminal: "sink".into(),
                position: Point { x: 6, y: 0 },
            },
            Command::BeginRoute {
                connection: "output-flow".into(),
                source_port: Some("output-belt-1".into()),
                target_port: None,
            },
        ] {
            assert!(outgoing.step(command).accepted);
        }
        let active = outgoing.active.as_ref().unwrap();
        assert_eq!(active.cells[0].x, active.target.x);
        let length = active.cells[0].y - active.target.y;
        assert!(
            outgoing
                .step(Command::Extend {
                    direction: Direction::North,
                    length,
                })
                .accepted
        );
        let view = RouteView::from(outgoing.active.as_ref().unwrap());
        let path_index = view.cells.len() - 1;
        let expected = outgoing.site_arm_use_view(0, view, path_index).unwrap();
        assert!(expected.external_outgoing);
        let admission = Admission::default();
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut future: Option<
            Pin<Box<dyn Future<Output = Result<SiteArmUse<'_>, RequestFailure>> + '_>>,
        > = Some(Box::pin(site_arm_use_admitted(
            &outgoing, 0, view, path_index, &admission,
        )));
        let AdmittedAdvance::Complete(Ok(actual)) =
            poll_admitted(&mut future, &admission, &mut allowance, 0)
        else {
            panic!("outgoing terminal arm must complete");
        };
        assert_eq!(actual.incoming, expected.incoming);
        assert_eq!(actual.outgoing, expected.outgoing);
        assert!(!actual.external_incoming);
        assert!(actual.external_outgoing);
        assert_eq!(
            allowance
                .usage()
                .categories
                .get(&WorkKind::CatalogVisits)
                .copied()
                .unwrap_or(0),
            0
        );
    }

    #[test]
    fn bridge_preparation_resumes_owner_route_and_catalog_visits() {
        use super::super::{Command, Limits};
        use aic_layout::logistics::TransportKind;
        use aic_layout::physical::{Block, Connection, Problem};

        let problem = Problem {
            blocks: ["west", "east", "south", "north"]
                .into_iter()
                .map(|id| Block {
                    id: id.into(),
                    facility: "grinder-1-mode-normal".into(),
                })
                .collect(),
            terminals: vec![],
            connections: vec![
                Connection {
                    id: "horizontal".into(),
                    source: "west".into(),
                    target: "east".into(),
                    commodity: "horizontal-item".into(),
                    transport: TransportKind::Belt,
                    rate: Rate {
                        numerator: 1,
                        denominator: 2,
                    },
                },
                Connection {
                    id: "vertical".into(),
                    source: "south".into(),
                    target: "north".into(),
                    commodity: "vertical-item".into(),
                    transport: TransportKind::Belt,
                    rate: Rate {
                        numerator: 1,
                        denominator: 4,
                    },
                },
            ],
        };
        let mut source = Session::new(
            problem,
            crate::constructive::tests::catalogs(),
            Limits {
                max_turns: 32,
                max_extension: 16,
            },
        )
        .unwrap();
        for (block, position, rotation) in [
            ("west", Point { x: 2, y: 5 }, 90),
            ("east", Point { x: 8, y: 5 }, 90),
            ("south", Point { x: 5, y: 9 }, 0),
            ("north", Point { x: 5, y: 1 }, 0),
        ] {
            assert!(
                source
                    .step(Command::Place {
                        block: block.into(),
                        position,
                        rotation
                    })
                    .accepted
            );
        }
        assert!(
            source
                .step(Command::BeginRoute {
                    connection: "horizontal".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        assert!(
            source
                .step(Command::Extend {
                    direction: Direction::East,
                    length: 2
                })
                .accepted
        );
        assert!(source.step(Command::Connect).accepted);
        let horizontal_cells = source.routes[0].as_ref().unwrap().cells.clone();
        let crossing = horizontal_cells[1];
        let option = aic_routing::crossing_options(
            &[aic_routing::Carrier {
                id: 0,
                network: 0,
                transport: TransportKind::Belt,
                rate: source.problem.connections[0].rate,
                cells: &horizontal_cells,
            }],
            &aic_routing::RouteJob {
                id: 1,
                transport: TransportKind::Belt,
                rate: source.problem.connections[1].rate,
            },
            &source.catalogs.components,
            &indexmap::IndexSet::new(),
        )
        .get(&(crossing, 3))
        .cloned()
        .unwrap();
        assert!(
            source
                .step(Command::BeginRoute {
                    connection: "vertical".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        let start = source.active.as_ref().unwrap().cells[0];
        assert!(
            source
                .step(Command::Extend {
                    direction: Direction::North,
                    length: start.y - crossing.y - 1,
                })
                .accepted
        );
        let mut baseline = source.clone();
        assert!(
            baseline
                .step(Command::ReserveBridge {
                    position: crossing,
                    component: option.component.clone(),
                    rotation: option.rotation,
                    crossed_axis: BridgeAxis::Horizontal,
                })
                .accepted
        );

        let admission = Admission::default();
        let future_admission = admission.clone();
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut future: Option<
            Pin<Box<dyn Future<Output = Result<PreparedBridge, RequestFailure>> + '_>>,
        > = Some(Box::pin(prepare_reserve_bridge(
            &source,
            crossing,
            &option.component,
            option.rotation,
            BridgeAxis::Horizontal,
            &future_admission,
        )));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        for kind in [
            WorkKind::ReplayUsers,
            WorkKind::ReplayRouteCells,
            WorkKind::CatalogVisits,
        ] {
            quotas[CUMULATIVE_WORK_KINDS
                .iter()
                .position(|candidate| *candidate == kind)
                .unwrap()] = 1;
        }
        let mut pages = 0;
        let prepared = loop {
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = poll_admitted(&mut future, &admission, &mut allowance, 0);
            allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    pages += 1;
                }
                AdmittedAdvance::Complete(Ok(prepared)) => break prepared,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("valid bridge failed: {failure:?}")
                }
            }
        };
        drop(future);
        assert!(pages >= 2);
        let expected = baseline
            .active
            .as_ref()
            .unwrap()
            .bridge_reservations
            .last()
            .unwrap();
        assert_eq!(prepared.position, expected.position);
        assert_eq!(prepared.component, expected.component);
        assert_eq!(prepared.rotation, expected.rotation);
        assert_eq!(prepared.crossed_axis, expected.crossed_axis);
        assert_eq!(prepared.direction, expected.direction);
        assert!(
            source
                .active
                .as_ref()
                .unwrap()
                .bridge_reservations
                .is_empty()
        );
        assert!(
            allowance.usage().categories[&WorkKind::ReplayRouteCells] >= horizontal_cells.len() - 1
        );
        let paged_work = allowance.usage().categories;
        let bytes = admission.0.borrow().reserved_bytes;
        assert_eq!(bytes, option.component.len());
        drop(prepared);
        allowance.release_peak(bytes);

        let baseline_admission = Admission::default();
        let mut baseline_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut baseline_future: Option<
            Pin<Box<dyn Future<Output = Result<PreparedBridge, RequestFailure>> + '_>>,
        > = Some(Box::pin(prepare_reserve_bridge(
            &source,
            crossing,
            &option.component,
            option.rotation,
            BridgeAxis::Horizontal,
            &baseline_admission,
        )));
        let AdmittedAdvance::Complete(Ok(unpaged)) = poll_admitted(
            &mut baseline_future,
            &baseline_admission,
            &mut baseline_allowance,
            0,
        ) else {
            panic!("unpaged bridge preparation must complete");
        };
        assert_eq!(paged_work, baseline_allowance.usage().categories);
        let mut fork = source.clone();
        let mut ledger = ActiveStorageLedger::for_fork(&fork);
        let mut commit: Option<Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>> =
            Some(Box::pin(commit_reserve_bridge(
                &mut fork,
                unpaged,
                &mut ledger,
                &baseline_admission,
            )));
        let AdmittedAdvance::Complete(Ok(())) =
            poll_admitted(&mut commit, &baseline_admission, &mut baseline_allowance, 0)
        else {
            panic!("private bridge commit must complete");
        };
        drop(commit);
        assert_eq!(
            fork.active.as_ref().unwrap().bridge_reservations,
            baseline.active.as_ref().unwrap().bridge_reservations
        );
        assert_eq!(
            baseline_admission.0.borrow().reserved_bytes,
            option.component.len() + ledger.bridge_bytes
        );
        drop(fork);
        baseline_allowance.release_peak(baseline_admission.0.borrow().reserved_bytes);

        let invalid_admission = Admission::default();
        let mut invalid_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut invalid_future: Option<
            Pin<Box<dyn Future<Output = Result<PreparedBridge, RequestFailure>> + '_>>,
        > = Some(Box::pin(prepare_reserve_bridge(
            &source,
            crossing,
            "missing-bridge",
            option.rotation,
            BridgeAxis::Horizontal,
            &invalid_admission,
        )));
        let AdmittedAdvance::Complete(Err(RequestFailure::Invalid(diagnostic))) = poll_admitted(
            &mut invalid_future,
            &invalid_admission,
            &mut invalid_allowance,
            0,
        ) else {
            panic!("unknown bridge component must be native-invalid");
        };
        assert_eq!(diagnostic.code, "constructive.bridge-reservation");
        assert_eq!(invalid_admission.0.borrow().reserved_bytes, 0);

        // Reuse the committed bridge with a second route on the same axis.
        // Keep the exact pre-crossing route as the active source so this probes
        // the installed-owner scan without unrelated endpoint selection.
        let prior_active = source.active.as_ref().unwrap().clone();
        assert!(
            baseline
                .step(Command::Extend {
                    direction: Direction::North,
                    length: 1,
                })
                .accepted
        );
        let target = baseline.active.as_ref().unwrap().target;
        assert!(
            baseline
                .step(Command::Extend {
                    direction: Direction::North,
                    length: crossing.y - target.y,
                })
                .accepted
        );
        {
            let active = baseline.active.as_ref().unwrap();
            let view = RouteView::from(active);
            let reserved = &active.bridge_reservations[0];
            let route_admission = Admission::default();
            let mut route_allowance = RequestAllowance::new(AllowanceCaps::default());
            let mut route_future: Option<
                Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>,
            > = Some(Box::pin(check_bridge_route_admitted(
                view,
                reserved,
                "vertical",
                &route_admission,
            )));
            let mut route_quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
            route_quotas[CUMULATIVE_WORK_KINDS
                .iter()
                .position(|kind| *kind == WorkKind::ReplayRouteCells)
                .unwrap()] = 1;
            let mut route_pages = 0;
            loop {
                route_allowance.begin_page_window(route_quotas, Duration::from_secs(10));
                let result =
                    poll_admitted(&mut route_future, &route_admission, &mut route_allowance, 0);
                route_allowance.end_page_window();
                match result {
                    AdmittedAdvance::Pending(stop) => {
                        assert_eq!(stop.reason, "page-quantum");
                        route_pages += 1;
                    }
                    AdmittedAdvance::Complete(Ok(())) => break,
                    AdmittedAdvance::Complete(Err(failure)) => {
                        panic!("valid bridge route failed: {failure:?}")
                    }
                }
            }
            assert!(route_pages > 0);
            let unpaged_admission = Admission::default();
            let mut unpaged_allowance = RequestAllowance::new(AllowanceCaps::default());
            let mut unpaged_future: Option<
                Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>,
            > = Some(Box::pin(check_bridge_route_admitted(
                view,
                reserved,
                "vertical",
                &unpaged_admission,
            )));
            let AdmittedAdvance::Complete(Ok(())) = poll_admitted(
                &mut unpaged_future,
                &unpaged_admission,
                &mut unpaged_allowance,
                0,
            ) else {
                panic!("unpaged bridge route must complete");
            };
            assert_eq!(
                route_allowance.usage().categories,
                unpaged_allowance.usage().categories
            );

            let expected_junctions = baseline.junction_updates(1, view).unwrap();
            let junction_admission = Admission::default();
            let mut junction_allowance = RequestAllowance::new(AllowanceCaps::default());
            let mut junction_future: Option<
                Pin<Box<dyn Future<Output = Result<PreparedJunctionUpdates, RequestFailure>> + '_>>,
            > = Some(Box::pin(prepare_junction_updates(
                &baseline,
                1,
                view,
                &junction_admission,
            )));
            let mut junction_quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
            for (kind, quota) in [
                (WorkKind::ReplayRouteCells, 1),
                (WorkKind::ReplayUsers, 1),
                (WorkKind::CatalogVisits, 1),
                (WorkKind::CopiedBytes, 64),
            ] {
                junction_quotas[CUMULATIVE_WORK_KINDS
                    .iter()
                    .position(|candidate| *candidate == kind)
                    .unwrap()] = quota;
            }
            let mut junction_pages = 0;
            let actual_junctions = loop {
                junction_allowance.begin_page_window(junction_quotas, Duration::from_secs(10));
                let result = poll_admitted(
                    &mut junction_future,
                    &junction_admission,
                    &mut junction_allowance,
                    0,
                );
                junction_allowance.end_page_window();
                match result {
                    AdmittedAdvance::Pending(stop) => {
                        assert_eq!(stop.reason, "page-quantum");
                        junction_pages += 1;
                    }
                    AdmittedAdvance::Complete(Ok(updates)) => break updates,
                    AdmittedAdvance::Complete(Err(failure)) => {
                        panic!("valid staged junctions failed: {failure:?}")
                    }
                }
            };
            assert!(junction_pages > 0);
            assert_eq!(actual_junctions.updates, expected_junctions);
            let paged_new_sites = actual_junctions.new_occupied_sites;
            let paged_junction_work = junction_allowance.usage().categories;
            drop(actual_junctions);
            junction_allowance.release_peak(junction_admission.0.borrow().reserved_bytes);
            let unpaged_junction_admission = Admission::default();
            let mut unpaged_junction_allowance = RequestAllowance::new(AllowanceCaps::default());
            let mut unpaged_junction_future: Option<
                Pin<Box<dyn Future<Output = Result<PreparedJunctionUpdates, RequestFailure>> + '_>>,
            > = Some(Box::pin(prepare_junction_updates(
                &baseline,
                1,
                view,
                &unpaged_junction_admission,
            )));
            let AdmittedAdvance::Complete(Ok(unpaged_junctions)) = poll_admitted(
                &mut unpaged_junction_future,
                &unpaged_junction_admission,
                &mut unpaged_junction_allowance,
                0,
            ) else {
                panic!("unpaged junction updates must complete");
            };
            assert_eq!(unpaged_junctions.updates, expected_junctions);
            assert_eq!(paged_new_sites, unpaged_junctions.new_occupied_sites);
            assert_eq!(
                paged_junction_work,
                unpaged_junction_allowance.usage().categories
            );
            drop(unpaged_junctions);
            unpaged_junction_allowance
                .release_peak(unpaged_junction_admission.0.borrow().reserved_bytes);

            let mut invalid_connect = baseline.clone();
            let reserved = &mut invalid_connect.active.as_mut().unwrap().bridge_reservations[0];
            reserved.direction = (reserved.direction + 1) % 4;
            let invalid_connect_admission = Admission::default();
            let mut invalid_connect_allowance = RequestAllowance::new(AllowanceCaps::default());
            let mut invalid_connect_future: Option<
                Pin<Box<dyn Future<Output = Result<PreparedConnect, RequestFailure>> + '_>>,
            > = Some(Box::pin(prepare_connect(
                &invalid_connect,
                &invalid_connect_admission,
            )));
            let AdmittedAdvance::Complete(Err(RequestFailure::Invalid(diagnostic))) = poll_admitted(
                &mut invalid_connect_future,
                &invalid_connect_admission,
                &mut invalid_connect_allowance,
                0,
            ) else {
                panic!("invalid bridge must reject prepared Connect");
            };
            assert_eq!(diagnostic.code, "constructive.bridge-turn");
            assert_eq!(invalid_connect_admission.0.borrow().reserved_bytes, 0);
            assert_eq!(invalid_connect_allowance.reserved_bytes(), 0);

            let expected_bridges = baseline.bridge_updates(1, view).unwrap();
            let bridge_admission = Admission::default();
            let mut bridge_allowance = RequestAllowance::new(AllowanceCaps::default());
            let mut bridge_future: Option<
                Pin<Box<dyn Future<Output = Result<Vec<Bridge>, RequestFailure>> + '_>>,
            > = Some(Box::pin(prepare_bridge_updates(
                &baseline,
                1,
                view,
                &bridge_admission,
            )));
            let mut bridge_quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
            for (kind, quota) in [
                (WorkKind::ReplayRouteCells, 1),
                (WorkKind::ReplayUsers, 1),
                (WorkKind::CopiedBytes, 64),
            ] {
                bridge_quotas[CUMULATIVE_WORK_KINDS
                    .iter()
                    .position(|candidate| *candidate == kind)
                    .unwrap()] = quota;
            }
            let mut bridge_pages = 0;
            let actual_bridges = loop {
                bridge_allowance.begin_page_window(bridge_quotas, Duration::from_secs(10));
                let result = poll_admitted(
                    &mut bridge_future,
                    &bridge_admission,
                    &mut bridge_allowance,
                    0,
                );
                bridge_allowance.end_page_window();
                match result {
                    AdmittedAdvance::Pending(stop) => {
                        assert_eq!(stop.reason, "page-quantum");
                        bridge_pages += 1;
                    }
                    AdmittedAdvance::Complete(Ok(bridges)) => break bridges,
                    AdmittedAdvance::Complete(Err(failure)) => {
                        panic!("valid staged bridges failed: {failure:?}")
                    }
                }
            };
            assert!(bridge_pages > 0);
            assert_eq!(actual_bridges, expected_bridges);
            let paged_work = bridge_allowance.usage().categories;
            drop(actual_bridges);
            bridge_allowance.release_peak(bridge_admission.0.borrow().reserved_bytes);
            let unpaged_admission = Admission::default();
            let mut unpaged_allowance = RequestAllowance::new(AllowanceCaps::default());
            let mut unpaged_future: Option<
                Pin<Box<dyn Future<Output = Result<Vec<Bridge>, RequestFailure>> + '_>>,
            > = Some(Box::pin(prepare_bridge_updates(
                &baseline,
                1,
                view,
                &unpaged_admission,
            )));
            let AdmittedAdvance::Complete(Ok(unpaged)) = poll_admitted(
                &mut unpaged_future,
                &unpaged_admission,
                &mut unpaged_allowance,
                0,
            ) else {
                panic!("unpaged bridge updates must complete");
            };
            assert_eq!(unpaged, expected_bridges);
            assert_eq!(paged_work, unpaged_allowance.usage().categories);
            drop(unpaged);
            unpaged_allowance.release_peak(unpaged_admission.0.borrow().reserved_bytes);

            // Exercise sorting when committed carrier visit order differs from
            // connection ID order; a single-owner crossing cannot expose it.
            let mut multi_owner = baseline.clone();
            let mut earlier = multi_owner.problem.connections[0].clone();
            earlier.id = "aaa-carrier".into();
            let earlier_index = multi_owner.problem.connections.len();
            Arc::make_mut(&mut multi_owner.problem)
                .connections
                .push(earlier);
            multi_owner.occupied[layer(TransportKind::Belt)]
                .get_mut(&crossing)
                .unwrap()
                .push(earlier_index);
            let multi_view = RouteView::from(multi_owner.active.as_ref().unwrap());
            let expected_multi = multi_owner.bridge_updates(1, multi_view).unwrap();
            assert_eq!(
                expected_multi[0].channels[0].connections,
                ["aaa-carrier", "horizontal"]
            );
            let multi_admission = Admission::default();
            let mut multi_allowance = RequestAllowance::new(AllowanceCaps::default());
            let mut multi_future: Option<
                Pin<Box<dyn Future<Output = Result<Vec<Bridge>, RequestFailure>> + '_>>,
            > = Some(Box::pin(prepare_bridge_updates(
                &multi_owner,
                1,
                multi_view,
                &multi_admission,
            )));
            let actual_multi = loop {
                multi_allowance.begin_page_window(bridge_quotas, Duration::from_secs(10));
                let result =
                    poll_admitted(&mut multi_future, &multi_admission, &mut multi_allowance, 0);
                multi_allowance.end_page_window();
                match result {
                    AdmittedAdvance::Pending(stop) => assert_eq!(stop.reason, "page-quantum"),
                    AdmittedAdvance::Complete(Ok(bridges)) => break bridges,
                    AdmittedAdvance::Complete(Err(failure)) => {
                        panic!("multi-owner staged bridge failed: {failure:?}")
                    }
                }
            };
            assert_eq!(actual_multi, expected_multi);
            drop(actual_multi);
            multi_allowance.release_peak(multi_admission.0.borrow().reserved_bytes);
        }
        assert!(baseline.step(Command::Connect).accepted);
        let crossing_commands = vec![
            Command::ReserveBridge {
                position: crossing,
                component: option.component.clone(),
                rotation: option.rotation,
                crossed_axis: BridgeAxis::Horizontal,
            },
            Command::Extend {
                direction: Direction::North,
                length: 1,
            },
            Command::Extend {
                direction: Direction::North,
                length: crossing.y - target.y,
            },
            Command::Connect,
        ];
        let mut unpaged_replay = NativeReplayProgress::new_borrowed(&source, &crossing_commands);
        let mut unpaged_allowance = RequestAllowance::new(AllowanceCaps::default());
        let ReplayAdvance::Complete(
            SourceOwner::Borrowed(returned),
            Ok((unpaged_child, unpaged_outcomes, unpaged_bytes)),
            unpaged_work,
        ) = unpaged_replay.advance(&mut unpaged_allowance, 0)
        else {
            panic!("valid crossing replay must complete");
        };
        assert!(std::ptr::eq(returned, &source));
        assert_eq!(unpaged_child.observe(), baseline.observe());
        assert_eq!(unpaged_child.metrics, baseline.metrics);
        assert_eq!(unpaged_outcomes.len(), crossing_commands.len());
        let mut paged_replay = NativeReplayProgress::new_borrowed(&source, &crossing_commands);
        let mut paged_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut crossing_quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        for kind in [WorkKind::ReplayRouteCells, WorkKind::ValidationRecords] {
            crossing_quotas[CUMULATIVE_WORK_KINDS
                .iter()
                .position(|candidate| *candidate == kind)
                .unwrap()] = 1;
        }
        let (paged_child, paged_outcomes, paged_bytes, paged_work) = loop {
            paged_allowance.begin_page_window(crossing_quotas, Duration::from_secs(10));
            let result = paged_replay.advance(&mut paged_allowance, 0);
            paged_allowance.end_page_window();
            match result {
                ReplayAdvance::Pending(stop) => assert_eq!(stop.reason, "page-quantum"),
                ReplayAdvance::Complete(
                    SourceOwner::Borrowed(returned),
                    Ok((child, outcomes, bytes)),
                    work,
                ) => {
                    assert!(std::ptr::eq(returned, &source));
                    break (child, outcomes, bytes, work);
                }
                _ => panic!("paged crossing replay must complete"),
            }
        };
        assert_eq!(paged_child.observe(), baseline.observe());
        assert_eq!(paged_child.metrics, baseline.metrics);
        assert_eq!(paged_outcomes, unpaged_outcomes);
        assert_eq!(paged_work, unpaged_work);
        drop((unpaged_child, unpaged_outcomes, paged_child, paged_outcomes));
        unpaged_allowance.release_peak(unpaged_bytes);
        paged_allowance.release_peak(paged_bytes);
        assert_eq!(unpaged_allowance.reserved_bytes(), 0);
        assert_eq!(paged_allowance.reserved_bytes(), 0);
        let mut installed_source = baseline;
        let mut extra = installed_source.problem.connections[1].clone();
        extra.id = "vertical-extra".into();
        Arc::make_mut(&mut installed_source.problem)
            .connections
            .push(extra);
        Arc::make_mut(&mut installed_source.connections).insert("vertical-extra".into(), 2);
        installed_source.routes.push(None);
        installed_source.active = Some(ActiveRoute {
            connection: 2,
            ..prior_active
        });
        installed_source.active_cells = source.active_cells.clone();
        installed_source.metrics = None;
        let mut ordinary = installed_source.clone();
        ordinary
            .reserve_bridge(
                crossing,
                &option.component,
                option.rotation,
                BridgeAxis::Horizontal,
            )
            .unwrap();
        let installed_admission = Admission::default();
        let mut installed_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut installed_future: Option<
            Pin<Box<dyn Future<Output = Result<PreparedBridge, RequestFailure>> + '_>>,
        > = Some(Box::pin(prepare_reserve_bridge(
            &installed_source,
            crossing,
            &option.component,
            option.rotation,
            BridgeAxis::Horizontal,
            &installed_admission,
        )));
        let mut installed_pages = 0;
        let installed = loop {
            installed_allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = poll_admitted(
                &mut installed_future,
                &installed_admission,
                &mut installed_allowance,
                0,
            );
            installed_allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    installed_pages += 1;
                }
                AdmittedAdvance::Complete(Ok(prepared)) => break prepared,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("installed bridge failed: {failure:?}")
                }
            }
        };
        assert!(installed_pages > 0);
        assert_eq!(
            installed.component,
            ordinary.active.as_ref().unwrap().bridge_reservations[0].component
        );
        assert!(installed_allowance.usage().categories[&WorkKind::ReplayRouteCells] > 0);
        let bytes = installed_admission.0.borrow().reserved_bytes;
        drop(installed);
        installed_allowance.release_peak(bytes);

        assert!(
            ordinary
                .step(Command::Extend {
                    direction: Direction::North,
                    length: 1,
                })
                .accepted
        );
        let target = ordinary.active.as_ref().unwrap().target;
        assert!(
            ordinary
                .step(Command::Extend {
                    direction: Direction::North,
                    length: crossing.y - target.y,
                })
                .accepted
        );
        let view = RouteView::from(ordinary.active.as_ref().unwrap());
        let expected = ordinary.bridge_updates(2, view).unwrap();
        let update_admission = Admission::default();
        let mut update_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut update_future: Option<
            Pin<Box<dyn Future<Output = Result<Vec<Bridge>, RequestFailure>> + '_>>,
        > = Some(Box::pin(prepare_bridge_updates(
            &ordinary,
            2,
            view,
            &update_admission,
        )));
        let mut update_quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        for (kind, quota) in [
            (WorkKind::ReplayRouteCells, 1),
            (WorkKind::ReplayUsers, 1),
            (WorkKind::CopiedBytes, 64),
        ] {
            update_quotas[CUMULATIVE_WORK_KINDS
                .iter()
                .position(|candidate| *candidate == kind)
                .unwrap()] = quota;
        }
        let mut update_pages = 0;
        let actual = loop {
            update_allowance.begin_page_window(update_quotas, Duration::from_secs(10));
            let result = poll_admitted(
                &mut update_future,
                &update_admission,
                &mut update_allowance,
                0,
            );
            update_allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    update_pages += 1;
                }
                AdmittedAdvance::Complete(Ok(bridges)) => break bridges,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("installed staged bridge failed: {failure:?}")
                }
            }
        };
        assert!(update_pages > 0);
        assert_eq!(actual, expected);
        assert_eq!(actual[0].channels[1].connections.len(), 2);
        let mut fork = ordinary.clone();
        let old_bridge = fork.bridges.get(&(TransportKind::Belt, crossing)).unwrap();
        let old_bridge_bytes =
            map_storage_bytes::<(TransportKind, Point), Bridge>(fork.bridges.capacity())
                + old_bridge.id.len()
                + old_bridge.component.len()
                + old_bridge
                    .channels
                    .iter()
                    .map(|channel| {
                        channel.commodity.len()
                            + channel.connections.capacity() * size_of::<String>()
                            + channel.connections.iter().map(String::len).sum::<usize>()
                    })
                    .sum::<usize>();
        update_allowance.reserve_peak(old_bridge_bytes).unwrap();
        update_admission.0.borrow_mut().reserved_bytes += old_bridge_bytes;
        let mut prepared = PreparedConnect {
            connection_index: 2,
            route_connection: String::new(),
            new_occupied_sites: 0,
            junctions: Vec::new(),
            bridges: actual,
        };
        let mut commit_future: Option<
            Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>,
        > = Some(Box::pin(commit_connect_components(
            &mut fork,
            &mut prepared,
            &update_admission,
        )));
        let mut commit_pages = 0;
        loop {
            update_allowance.begin_page_window(update_quotas, Duration::from_secs(10));
            let result = poll_admitted(
                &mut commit_future,
                &update_admission,
                &mut update_allowance,
                0,
            );
            update_allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    commit_pages += 1;
                }
                AdmittedAdvance::Complete(Ok(())) => break,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("installed bridge commit failed: {failure:?}")
                }
            }
        }
        drop(commit_future);
        assert!(commit_pages > 0);
        assert_eq!(fork.bridges[&(TransportKind::Belt, crossing)], expected[0]);
        assert!(prepared.bridges.is_empty());
        update_allowance.release_peak(update_admission.0.borrow().reserved_bytes);
    }

    #[test]
    fn private_connect_component_removal_releases_only_removed_heap() {
        use aic_layout::logistics::TransportKind;

        let mut fork = crate::constructive::tests::session();
        let site = (TransportKind::Belt, Point { x: 3, y: 4 });
        fork.junctions.insert(
            site,
            Junction {
                id: "junction-id".into(),
                component: "splitter".into(),
                position: site.1,
                rotation: 0,
                connections: vec!["one".into(), "two".into()],
            },
        );
        let old = &fork.junctions[&site];
        let map_bytes =
            map_storage_bytes::<(TransportKind, Point), Junction>(fork.junctions.capacity());
        let heap_bytes = old.id.len()
            + old.component.len()
            + old.connections.capacity() * size_of::<String>()
            + old.connections.iter().map(String::len).sum::<usize>();
        let mut junctions = Vec::with_capacity(1);
        junctions.push((site, None));
        let output_bytes = junctions.capacity() * size_of::<JunctionUpdate>();
        let mut prepared = PreparedConnect {
            connection_index: 0,
            route_connection: String::new(),
            new_occupied_sites: 0,
            junctions,
            bridges: Vec::new(),
        };
        let admission = Admission::default();
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        let baseline = map_bytes + heap_bytes + output_bytes;
        allowance.reserve_peak(baseline).unwrap();
        admission.0.borrow_mut().reserved_bytes = baseline;
        let mut future: Option<Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>> =
            Some(Box::pin(commit_connect_components(
                &mut fork,
                &mut prepared,
                &admission,
            )));
        assert!(matches!(
            poll_admitted(&mut future, &admission, &mut allowance, 0),
            AdmittedAdvance::Complete(Ok(()))
        ));
        drop(future);
        assert!(!fork.junctions.contains_key(&site));
        assert_eq!(admission.0.borrow().reserved_bytes, map_bytes);
        drop(fork);
        allowance.release_peak(map_bytes);
        assert_eq!(allowance.reserved_bytes(), 0);
    }

    #[test]
    fn private_route_command_sequence_matches_native_turns_and_progress() {
        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        let begin = Command::BeginRoute {
            connection: "connection-0".into(),
            source_port: Some("output-belt-1".into()),
            target_port: Some("input-belt-1".into()),
        };
        let mut ordinary = source.clone();
        let first = ordinary.step(begin.clone());
        assert!(first.accepted);
        let active = ordinary.active.as_ref().unwrap();
        let length = active.cells[0].y - active.target.y;
        let commands = [
            begin,
            Command::Extend {
                direction: Direction::North,
                length,
            },
            Command::Connect,
        ];
        let ordinary_steps = [
            first,
            ordinary.step(commands[1].clone()),
            ordinary.step(commands[2].clone()),
        ];
        assert!(ordinary_steps.iter().all(|step| step.accepted));
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut cursor = SessionForkCursor::new(&source);
        let (mut fork, copied_bytes) = match cursor.advance(&mut allowance, 0) {
            ForkAdvance::Complete(_, Ok(fork)) => fork,
            _ => panic!("small private route source must copy"),
        };
        drop(cursor);
        let admission = Admission::default();
        admission.0.borrow_mut().reserved_bytes = copied_bytes;
        let mut future: Option<
            Pin<
                Box<
                    dyn Future<Output = (Vec<ReplayCommandOutcome>, Result<(), RequestFailure>)>
                        + '_,
                >,
            >,
        > = Some(Box::pin(replay_route_commands(
            &mut fork, &commands, &admission,
        )));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        for kind in [WorkKind::ReplayRouteCells, WorkKind::CatalogVisits] {
            quotas[CUMULATIVE_WORK_KINDS
                .iter()
                .position(|candidate| *candidate == kind)
                .unwrap()] = 1;
        }
        let mut pages = 0;
        let outcomes = loop {
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = poll_admitted(&mut future, &admission, &mut allowance, 0);
            allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    pages += 1;
                }
                AdmittedAdvance::Complete((outcomes, Ok(()))) => break outcomes,
                AdmittedAdvance::Complete((_, Err(failure))) => {
                    panic!("private route sequence failed: {failure:?}")
                }
            }
        };
        drop(future);
        assert!(pages > 0);
        assert_eq!(outcomes.len(), ordinary_steps.len());
        for (actual, expected) in outcomes.iter().zip(ordinary_steps.iter()) {
            assert_eq!(actual.turn, expected.turn);
            assert_eq!(actual.revision, expected.revision);
            assert_eq!(actual.progress_before, expected.progress_before);
            assert_eq!(actual.progress_after, expected.progress_after);
            assert_eq!(actual.terminal, expected.terminal);
        }
        assert_eq!(fork.routes, ordinary.routes);
        assert_eq!(fork.occupied, ordinary.occupied);
        assert_eq!(fork.used_ports.len(), ordinary.used_ports.len());
        assert_eq!(fork.turns, ordinary.turns);
        assert_eq!(fork.revision, ordinary.revision);
        assert!(ordinary.metrics.is_some());
        let mut validation_future: Option<
            Pin<Box<dyn Future<Output = Result<Session, RequestFailure>>>>,
        > = Some(Box::pin(validate_completed_fork(fork, &admission)));
        let validation_index = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::ValidationRecords)
            .unwrap();
        quotas[validation_index] = 1;
        let validated = loop {
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = poll_admitted(&mut validation_future, &admission, &mut allowance, 0);
            allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => assert_eq!(stop.reason, "page-quantum"),
                AdmittedAdvance::Complete(Ok(child)) => break child,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("private final validation failed: {failure:?}")
                }
            }
        };
        assert_eq!(validated.metrics, ordinary.metrics);
        assert_eq!(validated.done(), ordinary.done());
        drop(validated);
        allowance.release_peak(admission.0.borrow().reserved_bytes);
        assert_eq!(allowance.reserved_bytes(), 0);

        let original_revision = source.revision;
        let mut retained = NativeReplayProgress::new_owned(source, commands.to_vec());
        let mut retained_allowance = RequestAllowance::new(AllowanceCaps::default());
        let (original, child, retained_outcomes, retained_bytes, paged_work) = loop {
            retained_allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = retained.advance(&mut retained_allowance, 0);
            retained_allowance.end_page_window();
            match result {
                ReplayAdvance::Pending(stop) => assert_eq!(stop.reason, "page-quantum"),
                ReplayAdvance::Complete(
                    SourceOwner::Owned(original),
                    Ok((child, outcomes, bytes)),
                    work,
                ) => {
                    break (original, child, outcomes, bytes, work);
                }
                _ => panic!("owned native replay must complete with original source"),
            }
        };
        assert_eq!(original.revision, original_revision);
        assert!(original.routes[0].is_none());
        assert_eq!(retained_outcomes.len(), ordinary_steps.len());
        assert_eq!(child.metrics, ordinary.metrics);
        assert_eq!(child.routes, ordinary.routes);
        drop(child);
        retained_allowance.release_peak(retained_bytes);
        assert_eq!(retained_allowance.reserved_bytes(), 0);

        let mut borrowed_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut borrowed = NativeReplayProgress::new_borrowed(&original, &commands);
        let ReplayAdvance::Complete(
            SourceOwner::Borrowed(returned),
            Ok((borrowed_child, _, borrowed_bytes)),
            unpaged_work,
        ) = borrowed.advance(&mut borrowed_allowance, 0)
        else {
            panic!("borrowed replay must return its exact source");
        };
        assert!(std::ptr::eq(returned, &original));
        assert_eq!(paged_work, unpaged_work);
        assert!(paged_work.validation > 0);
        assert_eq!(borrowed_child.metrics, ordinary.metrics);
        drop(borrowed_child);
        borrowed_allowance.release_peak(borrowed_bytes);
        drop(borrowed);

        let shared_source = Arc::new(original);
        let mut shared_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut shared = NativeReplayProgress::new_shared(shared_source.clone(), commands.to_vec());
        let ReplayAdvance::Complete(
            SourceOwner::Shared(returned),
            Ok((shared_child, _, shared_bytes)),
            _,
        ) = shared.advance(&mut shared_allowance, 0)
        else {
            panic!("shared replay must return its exact source");
        };
        assert!(Arc::ptr_eq(&returned, &shared_source));
        assert_eq!(shared_child.metrics, ordinary.metrics);
        drop(shared_child);
        shared_allowance.release_peak(shared_bytes);
        assert_eq!(shared_allowance.reserved_bytes(), 0);
    }

    #[test]
    fn owned_replay_discards_mid_connect_fork_on_hard_stop_and_cancel() {
        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        assert!(
            source
                .step(Command::BeginRoute {
                    connection: "connection-0".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        let active = source.active.as_ref().unwrap();
        let length = active.cells[0].y - active.target.y;
        assert!(
            source
                .step(Command::Extend {
                    direction: Direction::North,
                    length,
                })
                .accepted
        );
        let prep_admission = Admission::default();
        let mut prep_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut prep_future: Option<
            Pin<Box<dyn Future<Output = Result<PreparedConnect, RequestFailure>> + '_>>,
        > = Some(Box::pin(prepare_connect(&source, &prep_admission)));
        let AdmittedAdvance::Complete(Ok(prepared)) =
            poll_admitted(&mut prep_future, &prep_admission, &mut prep_allowance, 0)
        else {
            panic!("fixture Connect preparation must complete");
        };
        let prepared_route_visits = prep_allowance.usage().categories[&WorkKind::ReplayRouteCells];
        drop(prepared);
        drop(prep_future);
        prep_allowance.release_peak(prep_admission.0.borrow().reserved_bytes);

        let mut caps = AllowanceCaps::default();
        caps.replay_route_cells = prepared_route_visits + 1;
        let mut hard_allowance = RequestAllowance::new(caps);
        let original_revision = source.revision;
        let mut hard = NativeReplayProgress::new_owned(source, vec![Command::Connect]);
        let ReplayAdvance::Complete(
            SourceOwner::Owned(original),
            Err((RequestFailure::Stopped(_), outcomes, retained_bytes)),
            _,
        ) = hard.advance(&mut hard_allowance, 0)
        else {
            panic!("the second occupied-cell commit must hard-stop");
        };
        assert!(outcomes.is_empty());
        assert_eq!(retained_bytes, 0);
        assert_eq!(original.revision, original_revision);
        assert!(original.active.is_some());
        assert!(original.routes[0].is_none());
        assert_eq!(hard_allowance.reserved_bytes(), 0);

        let mut cancel_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        quotas[CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::ReplayRouteCells)
            .unwrap()] = prepared_route_visits + 1;
        cancel_allowance.begin_page_window(quotas, Duration::from_secs(10));
        let mut pending = NativeReplayProgress::new_owned(original, vec![Command::Connect]);
        assert!(matches!(
            pending.advance(&mut cancel_allowance, 0),
            ReplayAdvance::Pending(stop) if stop.reason == "page-quantum"
        ));
        assert!(pending.reserved_bytes() > 0);
        cancel_allowance.end_page_window();
        pending.cancel(&mut cancel_allowance);
        assert_eq!(cancel_allowance.reserved_bytes(), 0);
    }

    #[test]
    fn owned_replay_returns_accepted_prefix_with_its_reservation_on_invalid_command() {
        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        let original_revision = source.revision;
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut replay = NativeReplayProgress::new_owned(
            source,
            vec![
                Command::BeginRoute {
                    connection: "connection-0".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                },
                Command::Extend {
                    direction: Direction::North,
                    length: 0,
                },
            ],
        );
        let ReplayAdvance::Complete(
            SourceOwner::Owned(original),
            Err((RequestFailure::Invalid(_), outcomes, bytes)),
            _,
        ) = replay.advance(&mut allowance, 0)
        else {
            panic!("native-invalid second command must return the accepted prefix");
        };
        assert_eq!(original.revision, original_revision);
        assert!(original.active.is_none());
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].revision, original_revision + 1);
        assert_eq!(
            bytes,
            outcomes.capacity() * size_of::<ReplayCommandOutcome>()
        );
        assert_eq!(allowance.reserved_bytes(), bytes);
        drop(outcomes);
        allowance.release_peak(bytes);
        assert_eq!(allowance.reserved_bytes(), 0);
    }

    #[test]
    fn owned_network_partial_replay_resumes_without_replaying_first_member() {
        use aic_layout::physical::Block;

        let base = crate::constructive::tests::session();
        let mut problem = base.problem.as_ref().clone();
        problem.blocks.extend([
            Block {
                id: "source-2".into(),
                facility: "grinder-1-mode-normal".into(),
            },
            Block {
                id: "target-2".into(),
                facility: "grinder-1-mode-normal".into(),
            },
        ]);
        let mut second_connection = problem.connections[0].clone();
        second_connection.id = "connection-1".into();
        second_connection.source = "source-2".into();
        second_connection.target = "target-2".into();
        problem.connections.push(second_connection);
        let mut source = Session::new(problem, base.catalogs.clone(), base.limits).unwrap();
        for (block, position) in [
            ("source", Point { x: 5, y: 8 }),
            ("target", Point { x: 5, y: 2 }),
            ("source-2", Point { x: 13, y: 8 }),
            ("target-2", Point { x: 13, y: 2 }),
        ] {
            assert!(
                source
                    .step(Command::Place {
                        block: block.into(),
                        position,
                        rotation: 0
                    })
                    .accepted
            );
        }
        let first_begin = source.step(Command::BeginRoute {
            connection: "connection-0".into(),
            source_port: Some("output-belt-1".into()),
            target_port: Some("input-belt-1".into()),
        });
        assert!(first_begin.accepted, "{first_begin:?}");
        let first_length =
            source.active.as_ref().unwrap().cells[0].y - source.active.as_ref().unwrap().target.y;
        assert!(
            source
                .step(Command::Extend {
                    direction: Direction::North,
                    length: first_length
                })
                .accepted
        );
        assert!(source.step(Command::Connect).accepted);
        assert!(source.routes[0].is_some());
        assert!(source.routes[1].is_none());
        let commands = vec![
            Command::BeginRoute {
                connection: "connection-1".into(),
                source_port: Some("output-belt-1".into()),
                target_port: Some("input-belt-1".into()),
            },
            Command::Extend {
                direction: Direction::North,
                length: first_length,
            },
            Command::Connect,
        ];
        let mut ordinary = source.clone();
        for command in &commands {
            let step = ordinary.step(command.clone());
            assert!(step.accepted, "{step:?}");
        }
        let mut materialization_visits = 0;
        let mut layout_cursor = CompletedLayoutCursor::new(&ordinary);
        let MaterializationAdvance::Complete { layout, .. } = layout_cursor
            .advance(
                |count| {
                    materialization_visits += count;
                    count
                },
                |_| MaterializationReservation::Granted,
                |_| {},
            )
            .unwrap()
        else {
            panic!("unlimited terminal materialization must complete");
        };
        drop(layout);
        let mut unpaged = NativeReplayProgress::new_owned(source.clone(), commands.clone());
        let mut unpaged_allowance = RequestAllowance::new(AllowanceCaps::default());
        let ReplayAdvance::Complete(
            SourceOwner::Owned(_),
            Ok((unpaged_child, unpaged_outcomes, unpaged_bytes)),
            unpaged_work,
        ) = unpaged.advance(&mut unpaged_allowance, 0)
        else {
            panic!("unpaged second member must complete");
        };
        let source_revision = source.revision;
        let mut progress = NativeReplayProgress::new_owned(source, commands);
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut quotas = [64; CUMULATIVE_WORK_KINDS.len()];
        for kind in [
            WorkKind::ReplayRouteCells,
            WorkKind::CatalogVisits,
            WorkKind::ValidationRecords,
        ] {
            quotas[CUMULATIVE_WORK_KINDS
                .iter()
                .position(|item| *item == kind)
                .unwrap()] = 1;
        }
        let mut pages = 0;
        let mut stopped_kinds = std::collections::BTreeSet::new();
        let mut materialization_stopped = false;
        let mut validator_stopped = false;
        let (original, child, outcomes, bytes, paged_work) = loop {
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = progress.advance(&mut allowance, 0);
            allowance.end_page_window();
            match result {
                ReplayAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    if stop.category == Some(WorkKind::ValidationRecords) {
                        let visits = progress.admission.0.borrow().work.validation;
                        if visits < materialization_visits {
                            materialization_stopped = true;
                        } else {
                            validator_stopped = true;
                        }
                    }
                    stopped_kinds.insert(stop.category.unwrap());
                    pages += 1;
                }
                ReplayAdvance::Complete(
                    SourceOwner::Owned(original),
                    Ok((child, outcomes, bytes)),
                    work,
                ) => {
                    break (original, child, outcomes, bytes, work);
                }
                _ => panic!("second network member replay must complete"),
            }
        };
        assert!(pages > 0);
        assert!(stopped_kinds.contains(&WorkKind::CopiedBytes));
        assert!(stopped_kinds.contains(&WorkKind::ReplayRouteCells));
        assert!(stopped_kinds.contains(&WorkKind::ValidationRecords));
        assert!(materialization_stopped);
        assert!(validator_stopped);
        assert_eq!(original.revision, source_revision);
        assert!(original.routes[0].is_some());
        assert!(original.routes[1].is_none());
        assert_eq!(outcomes, unpaged_outcomes);
        assert_eq!(paged_work, unpaged_work);
        assert_eq!(bytes, unpaged_bytes);
        assert_eq!(child.observe(), ordinary.observe());
        assert_eq!(child.observe(), unpaged_child.observe());
        assert_eq!(child.metrics, ordinary.metrics);
        assert_eq!(child.metrics, unpaged_child.metrics);
        assert!(child.metrics.is_some());
        drop(unpaged_child);
        drop(unpaged_outcomes);
        unpaged_allowance.release_peak(unpaged_bytes);
        assert_eq!(unpaged_allowance.reserved_bytes(), 0);
        drop(child);
        drop(outcomes);
        allowance.release_peak(bytes);
        assert_eq!(allowance.reserved_bytes(), 0);
    }

    #[test]
    fn owned_repair_draft_source_resumes_native_member_replay() {
        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        assert!(
            source
                .step(Command::BeginRoute {
                    connection: "connection-0".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        let length =
            source.active.as_ref().unwrap().cells[0].y - source.active.as_ref().unwrap().target.y;
        assert!(
            source
                .step(Command::Extend {
                    direction: Direction::North,
                    length
                })
                .accepted
        );
        assert!(source.step(Command::Connect).accepted);
        let mut next = source.problem.connections[0].clone();
        next.id = "connection-1".into();
        Arc::make_mut(&mut source.problem).connections.push(next);
        Arc::make_mut(&mut source.connections).insert("connection-1".into(), 1);
        source.routes.push(None);
        source.metrics = None;
        let config = crate::constructive::tests::route_candidate_config();
        let request = source
            .repair_request(
                &["connection-1".into()],
                &["connection-0".into()],
                1,
                &config,
            )
            .unwrap();
        let mut allowance = RequestAllowance::new(request.allowance_caps.clone());
        let (draft, draft_bytes) = source
            .prepare_repair_draft_with_allowance(&request, &["connection-0".into()], &mut allowance)
            .unwrap();
        assert!(draft.routes.iter().all(Option::is_none));
        let draft_revision = draft.revision;
        let commands = vec![
            Command::BeginRoute {
                connection: "connection-1".into(),
                source_port: Some("output-belt-1".into()),
                target_port: Some("input-belt-1".into()),
            },
            Command::Extend {
                direction: Direction::North,
                length,
            },
            Command::Connect,
        ];
        let mut ordinary = draft.clone();
        for command in &commands {
            let step = ordinary.step(command.clone());
            assert!(step.accepted, "{step:?}");
        }
        let mut unpaged = NativeReplayProgress::new_owned(draft.clone(), commands.clone());
        let mut unpaged_allowance = RequestAllowance::new(request.allowance_caps.clone());
        let ReplayAdvance::Complete(
            SourceOwner::Owned(_),
            Ok((unpaged_child, unpaged_outcomes, unpaged_bytes)),
            unpaged_work,
        ) = unpaged.advance(&mut unpaged_allowance, 0)
        else {
            panic!("unpaged repair member must complete");
        };
        let mut progress = NativeReplayProgress::new_owned(draft, commands);
        let mut quotas = [64; CUMULATIVE_WORK_KINDS.len()];
        for kind in [WorkKind::ReplayRouteCells, WorkKind::CatalogVisits] {
            quotas[CUMULATIVE_WORK_KINDS
                .iter()
                .position(|item| *item == kind)
                .unwrap()] = 1;
        }
        let mut pages = 0;
        let mut stopped_kinds = std::collections::BTreeSet::new();
        let (original, child, outcomes, replay_bytes, paged_work) = loop {
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            // The draft reservation is already in this RequestAllowance.
            let result = progress.advance(&mut allowance, 0);
            allowance.end_page_window();
            match result {
                ReplayAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    stopped_kinds.insert(stop.category.unwrap());
                    pages += 1;
                }
                ReplayAdvance::Complete(
                    SourceOwner::Owned(original),
                    Ok((child, outcomes, bytes)),
                    work,
                ) => {
                    break (original, child, outcomes, bytes, work);
                }
                _ => panic!("repair draft member replay must complete"),
            }
        };
        assert!(pages > 0);
        assert!(stopped_kinds.contains(&WorkKind::CopiedBytes));
        assert!(stopped_kinds.contains(&WorkKind::ReplayRouteCells));
        assert_eq!(original.revision, draft_revision);
        assert!(original.routes.iter().all(Option::is_none));
        assert_eq!(outcomes, unpaged_outcomes);
        assert_eq!(paged_work, unpaged_work);
        assert_eq!(replay_bytes, unpaged_bytes);
        assert_eq!(child.observe(), ordinary.observe());
        assert_eq!(child.observe(), unpaged_child.observe());
        drop(unpaged_child);
        drop(unpaged_outcomes);
        unpaged_allowance.release_peak(unpaged_bytes);
        assert_eq!(unpaged_allowance.reserved_bytes(), 0);
        drop(child);
        drop(outcomes);
        allowance.release_peak(replay_bytes);
        drop(original);
        allowance.release_peak(draft_bytes);
        assert_eq!(allowance.reserved_bytes(), 0);
    }

    #[test]
    fn bridge_commit_moves_existing_reservations_across_pages() {
        use super::super::Command;

        let mut fork = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut fork);
        assert!(
            fork.step(Command::BeginRoute {
                connection: "connection-0".into(),
                source_port: Some("output-belt-1".into()),
                target_port: Some("input-belt-1".into()),
            })
            .accepted
        );
        let old = (0..8)
            .map(|index| BridgeReservation {
                position: Point { x: index, y: 30 },
                component: format!("old-{index}"),
                rotation: 0,
                crossed_axis: BridgeAxis::Horizontal,
                direction: 3,
            })
            .collect::<Vec<_>>();
        fork.active.as_mut().unwrap().bridge_reservations = old.clone();
        let mut ledger = ActiveStorageLedger::for_fork(&fork);
        let mut denied_fork = fork.clone();
        let mut denied_ledger = ActiveStorageLedger::for_fork(&denied_fork);
        let old_bytes = ledger.bridge_bytes;
        let new_component = "new-bridge";
        let prior_reservation = old_bytes
            + old
                .iter()
                .map(|record| record.component.len())
                .sum::<usize>()
            + new_component.len();
        let admission = Admission::default();
        admission.0.borrow_mut().reserved_bytes = prior_reservation;
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        allowance.reserve_peak(prior_reservation).unwrap();
        let prepared = PreparedBridge {
            position: Point { x: 8, y: 30 },
            component: new_component.into(),
            rotation: 0,
            crossed_axis: BridgeAxis::Horizontal,
            direction: 3,
        };
        let mut future: Option<Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>> =
            Some(Box::pin(commit_reserve_bridge(
                &mut fork,
                prepared,
                &mut ledger,
                &admission,
            )));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        quotas[CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap()] = size_of::<BridgeReservation>() * 2;
        let mut pages = 0;
        loop {
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = poll_admitted(&mut future, &admission, &mut allowance, 0);
            allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    pages += 1;
                }
                AdmittedAdvance::Complete(Ok(())) => break,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("private bridge commit failed: {failure:?}")
                }
            }
        }
        drop(future);
        assert!(pages >= 4);
        let reservations = &fork.active.as_ref().unwrap().bridge_reservations;
        assert_eq!(&reservations[..old.len()], old);
        assert_eq!(reservations.last().unwrap().component, new_component);
        assert_eq!(
            ledger.bridge_bytes,
            reservations.capacity() * size_of::<BridgeReservation>()
        );
        assert_eq!(
            admission.0.borrow().reserved_bytes,
            prior_reservation + ledger.bridge_bytes - old_bytes
        );
        drop(fork);
        allowance.release_peak(admission.0.borrow().reserved_bytes);
        assert_eq!(allowance.reserved_bytes(), 0);

        let denied_prior = denied_ledger.bridge_bytes
            + old
                .iter()
                .map(|record| record.component.len())
                .sum::<usize>()
            + new_component.len();
        let denied_admission = Admission::default();
        denied_admission.0.borrow_mut().reserved_bytes = denied_prior;
        let mut caps = AllowanceCaps::default();
        caps.peak_live_bytes = denied_prior + 9 * size_of::<BridgeReservation>() - 1;
        let mut denied_allowance = RequestAllowance::new(caps);
        denied_allowance.reserve_peak(denied_prior).unwrap();
        let denied_prepared = PreparedBridge {
            position: Point { x: 8, y: 30 },
            component: new_component.into(),
            rotation: 0,
            crossed_axis: BridgeAxis::Horizontal,
            direction: 3,
        };
        let mut denied_future: Option<
            Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>,
        > = Some(Box::pin(commit_reserve_bridge(
            &mut denied_fork,
            denied_prepared,
            &mut denied_ledger,
            &denied_admission,
        )));
        let AdmittedAdvance::Complete(Err(RequestFailure::Stopped(stop))) = poll_admitted(
            &mut denied_future,
            &denied_admission,
            &mut denied_allowance,
            0,
        ) else {
            panic!("impossible bridge growth must hard-stop");
        };
        assert_eq!(stop.reason, "peak-memory-limit");
        drop(denied_future);
        assert_eq!(
            denied_fork.active.as_ref().unwrap().bridge_reservations,
            old
        );
        drop(denied_fork);
        denied_allowance.release_peak(denied_admission.0.borrow().reserved_bytes);
        assert_eq!(denied_allowance.reserved_bytes(), 0);
    }

    #[test]
    fn retained_begin_route_resumes_catalog_visits_and_matches_native_commit() {
        use super::super::Command;

        let mut fork = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut fork);
        let mut baseline = fork.clone();
        assert!(
            baseline
                .step(Command::BeginRoute {
                    connection: "connection-0".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        let mut ledger = ActiveStorageLedger::for_fork(&fork);
        let budget = SearchMemoryBudget::new(1 << 20);
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        let admission = Admission::default();
        let future_admission = admission.clone();
        let mut future: Option<Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>> =
            Some(Box::pin(async {
                let prepared = prepare_begin_route(
                    &fork,
                    "connection-0",
                    Some("output-belt-1"),
                    Some("input-belt-1"),
                    &future_admission,
                )
                .await?;
                commit_begin_route(&mut fork, prepared, &mut ledger);
                Ok(())
            }));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        quotas[CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CatalogVisits)
            .unwrap()] = 1;
        quotas[CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap()] = 8;
        let mut pages = 0;
        loop {
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = poll_admitted(&mut future, &admission, &mut allowance, 0);
            allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    pages += 1;
                }
                AdmittedAdvance::Complete(Ok(())) => break,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("valid route start failed: {failure:?}")
                }
            }
        }
        drop(future);
        assert!(pages > 0);
        assert_eq!(fork.active, baseline.active);
        assert_eq!(fork.active_cells, baseline.active_cells);
        assert_eq!(fork.focus, baseline.focus);
        assert_eq!(ledger.vec_bytes, size_of::<Point>());
        assert_eq!(
            ledger.set_bytes,
            hash_bucket_count(1) * (size_of::<Point>() + 1)
        );
        assert_eq!(
            allowance.usage().categories[&WorkKind::CopiedBytes],
            "output-belt-1".len() + "input-belt-1".len()
        );
        allowance.release_peak(admission.0.borrow().reserved_bytes);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn retained_begin_route_rejects_unknown_port_before_mutation() {
        let mut fork = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut fork);
        let admission = Admission::default();
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut future: Option<
            Pin<Box<dyn Future<Output = Result<PreparedBeginRoute, RequestFailure>> + '_>>,
        > = Some(Box::pin(prepare_begin_route(
            &fork,
            "connection-0",
            Some("missing"),
            Some("input-belt-1"),
            &admission,
        )));
        let AdmittedAdvance::Complete(Err(RequestFailure::Invalid(error))) =
            poll_admitted(&mut future, &admission, &mut allowance, 0)
        else {
            panic!("unknown port must be native-invalid");
        };
        drop(future);
        assert_eq!(error.code, "constructive.unknown-port");
        assert!(fork.active.is_none());
        assert!(fork.active_cells.is_empty());
        assert_eq!(admission.0.borrow().reserved_bytes, 0);
    }

    #[test]
    fn retained_begin_route_releases_staged_ids_on_late_rejection() {
        use aic_layout::facilities::FacilityPortDirection;
        use aic_layout::logistics::TransportKind;

        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        for (case, (target_port, expected_code)) in [
            (Some("missing"), "constructive.unknown-port"),
            (Some("input-belt-1"), "constructive.used-port"),
        ]
        .into_iter()
        .enumerate()
        {
            if case == 1 {
                source.used_ports.insert(
                    (0, "output-belt-1".into()),
                    UsedPort {
                        commodity: "incompatible".into(),
                        transport: TransportKind::Belt,
                        direction: FacilityPortDirection::Output,
                        rate: source.problem.connections[0].rate,
                    },
                );
            }
            let admission = Admission::default();
            let mut allowance = RequestAllowance::new(AllowanceCaps::default());
            let mut future: Option<
                Pin<Box<dyn Future<Output = Result<PreparedBeginRoute, RequestFailure>> + '_>>,
            > = Some(Box::pin(prepare_begin_route(
                &source,
                "connection-0",
                Some("output-belt-1"),
                target_port,
                &admission,
            )));
            let AdmittedAdvance::Complete(Err(RequestFailure::Invalid(diagnostic))) =
                poll_admitted(&mut future, &admission, &mut allowance, 0)
            else {
                panic!("late invalidity must reject without a partial route");
            };
            drop(future);
            assert_eq!(diagnostic.code, expected_code);
            assert_eq!(admission.0.borrow().reserved_bytes, 0);
            assert!(source.active.is_none());
        }
    }

    #[test]
    fn retained_begin_then_extend_carries_container_ownership() {
        use super::super::Command;

        let mut fork = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut fork);
        let mut baseline = fork.clone();
        assert!(
            baseline
                .step(Command::BeginRoute {
                    connection: "connection-0".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        assert!(
            baseline
                .step(Command::Extend {
                    direction: Direction::North,
                    length: 2
                })
                .accepted
        );

        let mut ledger = ActiveStorageLedger::for_fork(&fork);
        let budget = SearchMemoryBudget::new(1 << 20);
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        let admission = Admission::default();
        let future_admission = admission.clone();
        let mut future: Option<Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>> =
            Some(Box::pin(async {
                let prepared = prepare_begin_route(
                    &fork,
                    "connection-0",
                    Some("output-belt-1"),
                    Some("input-belt-1"),
                    &future_admission,
                )
                .await?;
                commit_begin_route(&mut fork, prepared, &mut ledger);
                let prepared =
                    prepare_extend(&fork, Direction::North, 2, &future_admission).await?;
                commit_extend(&mut fork, prepared, &mut ledger, &future_admission).await
            }));
        assert!(matches!(
            poll_admitted(&mut future, &admission, &mut allowance, 0),
            AdmittedAdvance::Complete(Ok(()))
        ));
        drop(future);
        assert_eq!(fork.active, baseline.active);
        assert_eq!(fork.active_cells, baseline.active_cells);
        assert_eq!(fork.focus, baseline.focus);
        assert_eq!(allowance.usage().categories[&WorkKind::ReplayRouteCells], 2);
        assert_eq!(
            admission.0.borrow().reserved_bytes,
            "output-belt-1".len() + "input-belt-1".len() + ledger.vec_bytes + ledger.set_bytes
        );
        allowance.release_peak(admission.0.borrow().reserved_bytes);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn retained_begin_route_copies_long_selected_id_within_page_and_peak_budgets() {
        use aic_layout::facilities::ValidatedFacilityCatalog;
        use aic_layout::validation::Catalogs;

        let mut source = crate::constructive::tests::session();
        let long_id = "a".repeat(512);
        let mut facility_catalog = source.catalogs.facilities.catalog().clone();
        let facility = facility_catalog
            .facilities
            .iter_mut()
            .find(|facility| facility.id == "grinder-1-mode-normal")
            .unwrap();
        facility
            .ports
            .iter_mut()
            .find(|port| port.id == "output-belt-1")
            .unwrap()
            .id = long_id.clone();
        source.catalogs = Arc::new(Catalogs {
            identity: source.catalogs.identity.clone(),
            facilities: ValidatedFacilityCatalog::try_from_catalog(facility_catalog).unwrap(),
            transports: source.catalogs.transports.clone(),
            components: source.catalogs.components.clone(),
        });
        crate::constructive::tests::place_pair(&mut source);

        let mut ledger = ActiveStorageLedger::for_fork(&source);
        let budget = SearchMemoryBudget::new(700);
        let mut caps = AllowanceCaps::default();
        caps.peak_live_bytes = 700;
        let mut allowance = RequestAllowance::with_memory_budget(caps, Some(budget.clone()));
        let admission = Admission::default();
        let future_admission = admission.clone();
        let mut future: Option<Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>> =
            Some(Box::pin(async {
                let prepared = prepare_begin_route(
                    &source,
                    "connection-0",
                    Some(&long_id),
                    Some("input-belt-1"),
                    &future_admission,
                )
                .await?;
                commit_begin_route(&mut source, prepared, &mut ledger);
                Ok(())
            }));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        quotas[CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap()] = 64;
        let mut pages = 0;
        loop {
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = poll_admitted(&mut future, &admission, &mut allowance, 0);
            allowance.end_page_window();
            match result {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    pages += 1;
                }
                AdmittedAdvance::Complete(Ok(())) => break,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("long valid port failed: {failure:?}")
                }
            }
        }
        drop(future);
        assert!(pages >= 8);
        assert_eq!(
            source.active.as_ref().unwrap().source_port.as_deref(),
            Some(long_id.as_str())
        );
        assert_eq!(
            allowance.usage().categories[&WorkKind::CopiedBytes],
            long_id.len() + "input-belt-1".len()
        );
        assert!(budget.peak_bytes() <= 700);
        allowance.release_peak(admission.0.borrow().reserved_bytes);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn joint_extend_growth_preserves_a_feasible_second_replacement() {
        let plan = plan_joint_extend_growth(JointGrowthInput {
            needed_cells: 9,
            old_vec_capacity: 7,
            old_set_capacity: 7,
            old_vec_bytes: 7 * size_of::<Point>(),
            old_set_bytes: 8 * (size_of::<Point>() + 1),
            current_reserved_bytes: 7 * size_of::<Point>()
                + 8 * (size_of::<Point>() + 1)
                + 2 * size_of::<Point>(),
            stable_peak_cap: 600,
        })
        .expect("jointly sized replacements fit the peak");
        assert_eq!(plan.order, GrowthOrder::SetThenVec);
        assert!(plan.vec_capacity >= 9);
        assert!(plan.set_capacity >= 9);
        assert!(280 + plan.escrow_bytes <= 600);
    }

    #[test]
    fn joint_extend_growth_keeps_vector_slack_when_set_bucket_is_exact() {
        let plan = plan_joint_extend_growth(JointGrowthInput {
            needed_cells: 14,
            old_vec_capacity: 8,
            old_set_capacity: 7,
            old_vec_bytes: 8 * size_of::<Point>(),
            old_set_bytes: 8 * (size_of::<Point>() + 1),
            current_reserved_bytes: 8 * size_of::<Point>()
                + 8 * (size_of::<Point>() + 1)
                + 7 * size_of::<Point>(),
            stable_peak_cap: 2_000,
        })
        .expect("growth fits the peak");
        assert_eq!(plan.vec_capacity, 16);
        assert_eq!(plan.set_capacity, 14);
    }

    fn growth_spec() -> GrowthSpec {
        GrowthSpec {
            needed_cells: 9,
            old_vec_capacity: 7,
            old_set_capacity: 7,
            old_vec_bytes: 7 * size_of::<Point>(),
            old_set_bytes: 8 * (size_of::<Point>() + 1),
        }
    }

    #[test]
    fn joint_growth_replans_after_pinned_floor_changes_during_aggregate_retry() {
        let tree = SearchMemoryBudget::new(1_000);
        tree.reserve(300).unwrap(); // live source owner outside this allowance
        let mut caps = AllowanceCaps::default();
        caps.peak_live_bytes = 1_000;
        let mut allowance = RequestAllowance::with_memory_budget(caps, Some(tree.clone()));
        allowance.reserve_peak(280).unwrap(); // old containers and prepared cells
        tree.reserve(200).unwrap(); // releasable peer pressure

        let admission = Admission::default();
        let pending_admission = admission.clone();
        let mut future: Option<
            Pin<Box<dyn Future<Output = Result<JointGrowthPlan, RequestFailure>>>>,
        > = Some(Box::pin(async move {
            pending_admission.reserve_joint_growth(growth_spec()).await
        }));
        assert!(matches!(
            poll_admitted(&mut future, &admission, &mut allowance, 300),
            AdmittedAdvance::Pending(AllowanceStop {
                reason: "aggregate-memory-limit",
                ..
            })
        ));
        tree.reserve(100).unwrap(); // source owner grew while replay was pending
        tree.release(200);
        let AdmittedAdvance::Complete(Ok(plan)) =
            poll_admitted(&mut future, &admission, &mut allowance, 400)
        else {
            panic!("a smaller joint plan must complete after releasable pressure leaves");
        };
        assert!(plan.vec_capacity >= 9);
        assert_eq!(admission.0.borrow().reserved_bytes, plan.escrow_bytes);
        allowance.release_peak(plan.escrow_bytes + 280);
        tree.release(400);
        assert_eq!(tree.live_bytes(), 0);
    }

    #[test]
    fn joint_growth_does_not_retry_an_impossible_pinned_source_peak() {
        let tree = SearchMemoryBudget::new(1_000);
        tree.reserve(500).unwrap();
        let mut caps = AllowanceCaps::default();
        caps.peak_live_bytes = 1_000;
        let mut allowance = RequestAllowance::with_memory_budget(caps, Some(tree.clone()));
        allowance.reserve_peak(280).unwrap();

        let admission = Admission::default();
        let pending_admission = admission.clone();
        let mut future: Option<
            Pin<Box<dyn Future<Output = Result<JointGrowthPlan, RequestFailure>>>>,
        > = Some(Box::pin(async move {
            pending_admission.reserve_joint_growth(growth_spec()).await
        }));
        let AdmittedAdvance::Complete(Err(RequestFailure::Stopped(stop))) =
            poll_admitted(&mut future, &admission, &mut allowance, 500)
        else {
            panic!("pinned source makes even required growth impossible");
        };
        assert_eq!(stop.reason, "peak-memory-limit");
        assert_eq!(
            allowance.last_stop().as_ref().map(|stop| stop.reason),
            Some(stop.reason)
        );
        allowance.release_peak(280);
        tree.release(500);
        assert_eq!(tree.live_bytes(), 0);
    }

    fn populated_source() -> Session {
        let mut source = crate::constructive::tests::session();
        let cells = (0..400).map(|x| Point { x, y: 0 }).collect::<Vec<_>>();
        source.routes[0] = Some(Route {
            connection: "connection-0".into(),
            source_port: None,
            target_port: None,
            cells: cells.clone(),
        });
        for point in cells {
            source.occupied[0].insert(point, vec![0]);
        }
        source
    }

    #[test]
    fn shared_cell_rule_resumes_at_the_next_user_without_rescanning() {
        let mut source = crate::constructive::tests::session();
        let point = Point { x: 0, y: 0 };
        source.occupied[0].insert(point, vec![0, 0, 0, 0]);
        let expected = source.shared_cell_available(point, 0);
        let admission = Admission::default();
        let mut future: Option<Pin<Box<dyn Future<Output = Result<bool, RequestFailure>> + '_>>> =
            Some(Box::pin(shared_cell_available(
                &source, point, 0, &admission,
            )));
        let mut allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let users_index = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::ReplayUsers)
            .unwrap();
        quotas[users_index] = 2;
        allowance.begin_page_window(quotas, Duration::from_secs(10));
        assert!(matches!(
            poll_admitted(&mut future, &admission, &mut allowance, 0),
            AdmittedAdvance::Pending(stop) if stop.reason == "page-quantum"
        ));
        assert_eq!(allowance.usage().categories[&WorkKind::ReplayUsers], 2);
        allowance.end_page_window();
        let AdmittedAdvance::Complete(Ok(actual)) =
            poll_admitted(&mut future, &admission, &mut allowance, 0)
        else {
            panic!("shared-cell check must resume after its second user");
        };
        assert_eq!(actual, expected);
        assert_eq!(allowance.usage().categories[&WorkKind::ReplayUsers], 4);
    }

    #[test]
    fn extension_legality_resumes_inside_shared_users_without_mutating_source() {
        use super::super::Command;

        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        assert!(
            source
                .step(Command::BeginRoute {
                    connection: "connection-0".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        Arc::make_mut(&mut source.problem).connections[0].rate = aic_layout::rate::Rate {
            numerator: 1,
            denominator: 100,
        };
        source.occupied[0].insert(Point { x: 6, y: 6 }, vec![0, 0]);
        let original_cells = source.active.as_ref().unwrap().cells.clone();
        let mut baseline = source.clone();
        baseline.extend(Direction::North, 2).unwrap();
        let expected = baseline.active.as_ref().unwrap().cells[original_cells.len()..].to_vec();

        let budget = SearchMemoryBudget::new(1 << 20);
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        let admission = Admission::default();
        let future_admission = admission.clone();
        let mut future: Option<
            Pin<Box<dyn Future<Output = Result<Vec<Point>, RequestFailure>> + '_>>,
        > = Some(Box::pin(async {
            let prepared = prepare_extend(&source, Direction::North, 2, &future_admission).await?;
            let cells = prepared.cells.clone();
            let bytes = prepared.reserved_bytes;
            drop(prepared);
            future_admission.release(bytes).await;
            Ok(cells)
        }));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        for kind in [WorkKind::ReplayRouteCells, WorkKind::ReplayUsers] {
            let index = CUMULATIVE_WORK_KINDS
                .iter()
                .position(|candidate| *candidate == kind)
                .unwrap();
            quotas[index] = 1;
        }
        allowance.begin_page_window(quotas, Duration::from_secs(10));
        assert!(matches!(
            poll_admitted(&mut future, &admission, &mut allowance, 0),
            AdmittedAdvance::Pending(stop) if stop.reason == "page-quantum"
        ));
        allowance.end_page_window();
        let AdmittedAdvance::Complete(Ok(actual)) =
            poll_admitted(&mut future, &admission, &mut allowance, 0)
        else {
            panic!("extension must finish after its retained user scan resumes");
        };
        assert_eq!(actual, expected);
        assert_eq!(allowance.usage().categories[&WorkKind::ReplayRouteCells], 2);
        assert_eq!(allowance.usage().categories[&WorkKind::ReplayUsers], 2);
        assert_eq!(source.active.as_ref().unwrap().cells, original_cells);
        assert_eq!(admission.0.borrow().reserved_bytes, 0);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn rejected_extension_releases_its_private_scratch() {
        use super::super::Command;

        let mut source = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut source);
        assert!(
            source
                .step(Command::BeginRoute {
                    connection: "connection-0".into(),
                    source_port: Some("output-belt-1".into()),
                    target_port: Some("input-belt-1".into()),
                })
                .accepted
        );
        let original_cells = source.active.as_ref().unwrap().cells.clone();
        let budget = SearchMemoryBudget::new(1 << 20);
        for (direction, caps) in [
            (Direction::South, AllowanceCaps::default()),
            (Direction::North, {
                let mut caps = AllowanceCaps::default();
                caps.replay_route_cells = 0;
                caps
            }),
        ] {
            let mut allowance = RequestAllowance::with_memory_budget(caps, Some(budget.clone()));
            let admission = Admission::default();
            let mut future: Option<
                Pin<Box<dyn Future<Output = Result<PreparedExtend, RequestFailure>> + '_>>,
            > = Some(Box::pin(prepare_extend(&source, direction, 1, &admission)));
            let AdmittedAdvance::Complete(Err(failure)) =
                poll_admitted(&mut future, &admission, &mut allowance, 0)
            else {
                panic!("invalid or over-budget extension must be rejected");
            };
            match direction {
                Direction::South => assert!(matches!(failure, RequestFailure::Invalid(_))),
                Direction::North => assert!(matches!(failure, RequestFailure::Stopped(_))),
                _ => unreachable!(),
            }
            assert_eq!(admission.0.borrow().reserved_bytes, 0);
            assert_eq!(budget.live_bytes(), 0);
        }
        assert_eq!(source.active.as_ref().unwrap().cells, original_cells);
    }

    fn extend_growth_source() -> Session {
        use super::super::Command;
        let mut fork = crate::constructive::tests::session();
        crate::constructive::tests::place_pair(&mut fork);
        assert!(
            fork.step(Command::BeginRoute {
                connection: "connection-0".into(),
                source_port: Some("output-belt-1".into()),
                target_port: Some("input-belt-1".into()),
            })
            .accepted
        );
        fork.extend(Direction::North, 2).unwrap();
        fork
    }

    #[test]
    fn retained_extend_commit_matches_native_growth_and_carries_copy_progress() {
        let mut fork = extend_growth_source();
        let mut baseline = fork.clone();
        baseline.extend(Direction::East, 4).unwrap();
        let mut ledger = ActiveStorageLedger::for_fork(&fork);
        let base_bytes = ledger.vec_bytes + ledger.set_bytes;
        let budget = SearchMemoryBudget::new(1 << 20);
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        allowance.reserve_peak(base_bytes).unwrap();
        let admission = Admission::default();
        let future_admission = admission.clone();
        let mut future: Option<Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>> =
            Some(Box::pin(async {
                let prepared = prepare_extend(&fork, Direction::East, 4, &future_admission).await?;
                commit_extend(&mut fork, prepared, &mut ledger, &future_admission).await
            }));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        quotas[CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap()] = size_of::<Point>();
        allowance.begin_page_window(quotas, Duration::from_secs(10));
        let mut pages = 0;
        loop {
            match poll_admitted(&mut future, &admission, &mut allowance, 0) {
                AdmittedAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    pages += 1;
                    allowance.end_page_window();
                    allowance.begin_page_window(quotas, Duration::from_secs(10));
                }
                AdmittedAdvance::Complete(Ok(())) => break,
                AdmittedAdvance::Complete(Err(failure)) => {
                    panic!("retained extension failed: {failure:?}")
                }
            }
        }
        allowance.end_page_window();
        drop(future);
        assert!(pages >= 2, "both old containers must copy across pages");
        assert_eq!(
            fork.active.as_ref().unwrap().cells,
            baseline.active.as_ref().unwrap().cells
        );
        assert_eq!(fork.active_cells, baseline.active_cells);
        assert_eq!(fork.focus, baseline.focus);
        assert_eq!(allowance.usage().categories[&WorkKind::ReplayRouteCells], 4);
        assert_eq!(
            admission.0.borrow().reserved_bytes,
            ledger.vec_bytes + ledger.set_bytes - base_bytes
        );
        allowance.release_peak(base_bytes + admission.0.borrow().reserved_bytes);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn retained_extend_growth_releases_escrow_after_hard_stop() {
        let mut fork = extend_growth_source();
        let mut ledger = ActiveStorageLedger::for_fork(&fork);
        let base_bytes = ledger.vec_bytes + ledger.set_bytes;
        let budget = SearchMemoryBudget::new(1 << 20);
        let mut caps = AllowanceCaps::default();
        caps.copied_bytes = size_of::<Point>();
        let mut allowance = RequestAllowance::with_memory_budget(caps, Some(budget.clone()));
        allowance.reserve_peak(base_bytes).unwrap();
        let admission = Admission::default();
        let future_admission = admission.clone();
        let mut future: Option<Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>> =
            Some(Box::pin(async {
                let prepared = prepare_extend(&fork, Direction::East, 4, &future_admission).await?;
                commit_extend(&mut fork, prepared, &mut ledger, &future_admission).await
            }));
        let AdmittedAdvance::Complete(Err(RequestFailure::Stopped(stop))) =
            poll_admitted(&mut future, &admission, &mut allowance, 0)
        else {
            panic!("copy allowance must stop after the first old cell");
        };
        assert_eq!(stop.reason, "work-limit");
        drop(future);
        assert!(budget.peak_bytes() > base_bytes);
        allowance.release_peak(base_bytes + admission.0.borrow().reserved_bytes);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn retained_extend_keeps_existing_set_allocation_when_it_has_spare_capacity() {
        let mut fork = extend_growth_source();
        fork.active_cells.reserve(8);
        let initial_set_capacity = fork.active_cells.capacity();
        let mut baseline = fork.clone();
        baseline.extend(Direction::East, 1).unwrap();
        let mut ledger = ActiveStorageLedger::for_fork(&fork);
        let base_bytes = ledger.vec_bytes + ledger.set_bytes;
        let budget = SearchMemoryBudget::new(1 << 20);
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        allowance.reserve_peak(base_bytes).unwrap();
        let admission = Admission::default();
        let future_admission = admission.clone();
        let mut future: Option<Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>> =
            Some(Box::pin(async {
                let prepared = prepare_extend(&fork, Direction::East, 1, &future_admission).await?;
                commit_extend(&mut fork, prepared, &mut ledger, &future_admission).await
            }));
        assert!(matches!(
            poll_admitted(&mut future, &admission, &mut allowance, 0),
            AdmittedAdvance::Complete(Ok(()))
        ));
        drop(future);
        assert_eq!(fork.active_cells.capacity(), initial_set_capacity);
        assert_eq!(fork.active_cells, baseline.active_cells);
        assert_eq!(
            ledger.set_bytes,
            hash_bucket_count(initial_set_capacity) * (size_of::<Point>() + 1)
        );
        allowance.release_peak(base_bytes + admission.0.borrow().reserved_bytes);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn retained_extend_executes_joint_set_first_growth_under_tight_peak() {
        let mut fork = extend_growth_source();
        let cells = [
            Point { x: 6, y: 8 },
            Point { x: 6, y: 7 },
            Point { x: 6, y: 6 },
            Point { x: 6, y: 5 },
            Point { x: 7, y: 5 },
            Point { x: 8, y: 5 },
            Point { x: 8, y: 6 },
        ];
        fork.active.as_mut().unwrap().cells = cells.to_vec();
        fork.active_cells = cells.into_iter().collect();
        assert_eq!(fork.active.as_ref().unwrap().cells.capacity(), 7);
        assert_eq!(fork.active_cells.capacity(), 7);
        let mut baseline = fork.clone();
        baseline.extend(Direction::East, 2).unwrap();
        let mut ledger = ActiveStorageLedger::for_fork(&fork);
        let base_bytes = ledger.vec_bytes + ledger.set_bytes;
        let mut caps = AllowanceCaps::default();
        caps.peak_live_bytes = 600;
        let budget = SearchMemoryBudget::new(600);
        let mut allowance = RequestAllowance::with_memory_budget(caps, Some(budget.clone()));
        allowance.reserve_peak(base_bytes).unwrap();
        let admission = Admission::default();
        let future_admission = admission.clone();
        let mut future: Option<Pin<Box<dyn Future<Output = Result<(), RequestFailure>> + '_>>> =
            Some(Box::pin(async {
                let prepared = prepare_extend(&fork, Direction::East, 2, &future_admission).await?;
                commit_extend(&mut fork, prepared, &mut ledger, &future_admission).await
            }));
        assert!(matches!(
            poll_admitted(&mut future, &admission, &mut allowance, 0),
            AdmittedAdvance::Complete(Ok(()))
        ));
        drop(future);
        assert_eq!(
            fork.active.as_ref().unwrap().cells,
            baseline.active.as_ref().unwrap().cells
        );
        assert_eq!(fork.active_cells, baseline.active_cells);
        assert!(budget.peak_bytes() <= 600);
        allowance.release_peak(base_bytes + admission.0.borrow().reserved_bytes);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn fork_copy_keeps_route_and_occupancy_across_page_and_releases_memory() {
        let source = populated_source();
        let budget = SearchMemoryBudget::new(1 << 26);
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let copied_index = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap();
        quotas[copied_index] = 512;
        allowance.begin_page_window(quotas, Duration::from_secs(10));
        let mut cursor = SessionForkCursor::new(&source);
        assert!(matches!(
            cursor.advance(&mut allowance, 0),
            ForkAdvance::Pending(stop) if stop.reason == "page-quantum"
        ));
        assert!(cursor.reserved_bytes() > 0);
        assert!(budget.live_bytes() > 0);
        allowance.end_page_window();
        let ForkAdvance::Complete(SourceOwner::Borrowed(original), Ok((fork, reserved))) =
            cursor.advance(&mut allowance, 0)
        else {
            panic!("fork must complete when the temporary page ends");
        };
        assert!(std::ptr::eq(original, &source));
        assert!(std::sync::Arc::ptr_eq(&source.problem, &fork.problem));
        assert_eq!(fork.routes, source.routes);
        assert_eq!(fork.occupied, source.occupied);
        assert_ne!(fork.instance_id.0, source.instance_id.0);
        assert_eq!(budget.live_bytes(), reserved);
        drop(fork);
        allowance.release_peak(reserved);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn cancelled_fork_copy_releases_its_partial_reservation() {
        let source = populated_source();
        let budget = SearchMemoryBudget::new(1 << 26);
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let copied_index = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap();
        quotas[copied_index] = 512;
        allowance.begin_page_window(quotas, Duration::from_secs(10));
        let mut cursor = SessionForkCursor::new(&source);
        assert!(matches!(
            cursor.advance(&mut allowance, 0),
            ForkAdvance::Pending(_)
        ));
        allowance.end_page_window();
        cursor.cancel(&mut allowance);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn shared_and_moved_sources_survive_fork_yield_and_hard_stop() {
        let source = Arc::new(populated_source());
        let budget = SearchMemoryBudget::new(1 << 26);
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let copied_index = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap();
        quotas[copied_index] = 128;
        allowance.begin_page_window(quotas, Duration::from_secs(10));
        let mut shared = SessionForkCursor::new_shared(source.clone());
        assert!(matches!(
            shared.advance(&mut allowance, 0),
            ForkAdvance::Pending(stop) if stop.reason == "page-quantum"
        ));
        allowance.end_page_window();
        let ForkAdvance::Complete(SourceOwner::Shared(returned), Ok((fork, bytes))) =
            shared.advance(&mut allowance, 0)
        else {
            panic!("shared source and fork must return together");
        };
        assert!(Arc::ptr_eq(&source, &returned));
        assert_eq!(fork.routes, source.routes);
        drop(fork);
        allowance.release_peak(bytes);
        assert_eq!(budget.live_bytes(), 0);

        let original = populated_source();
        let original_id = original.instance_id.0;
        let mut caps = AllowanceCaps::default();
        caps.peak_live_bytes = 4_096;
        let mut hard_allowance = RequestAllowance::with_memory_budget(caps, Some(budget.clone()));
        let mut moved = SessionForkCursor::new_owned(original);
        let ForkAdvance::Complete(SourceOwner::Owned(returned), Err(RequestFailure::Stopped(stop))) =
            moved.advance(&mut hard_allowance, 0)
        else {
            panic!("hard stop must return the moved original source");
        };
        assert_eq!(stop.reason, "peak-memory-limit");
        assert_eq!(returned.instance_id.0, original_id);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn fixed_bridge_record_copy_completes_across_repeated_small_pages() {
        use aic_layout::logistics::TransportKind;
        use aic_layout::physical::BridgeAxis;

        let mut source = crate::constructive::tests::session();
        let point = Point { x: 0, y: 0 };
        source.bridges.insert(
            (TransportKind::Belt, point),
            Bridge {
                id: "bridge-belt-0-0".into(),
                component: "bridge-belt".into(),
                position: point,
                rotation: 0,
                channels: [BridgeAxis::Horizontal, BridgeAxis::Vertical].map(|axis| {
                    BridgeChannel {
                        axis,
                        commodity: "test-item".into(),
                        connections: vec!["connection-0".into()],
                    }
                }),
            },
        );
        let budget = SearchMemoryBudget::new(1 << 26);
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let copied_index = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap();
        quotas[copied_index] = 128;
        let mut cursor = SessionForkCursor::new(&source);
        let mut page_count = 0;
        let (fork, reserved) = loop {
            assert!(page_count < 200, "valid bridge copy must progress by page");
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = cursor.advance(&mut allowance, 0);
            allowance.end_page_window();
            page_count += 1;
            match result {
                ForkAdvance::Pending(stop) => assert_eq!(stop.reason, "page-quantum"),
                ForkAdvance::Complete(_, Ok(result)) => break result,
                ForkAdvance::Complete(_, Err(failure)) => panic!("copy failed: {failure:?}"),
            }
        };
        assert!(page_count > 1);
        assert_eq!(fork.bridges, source.bridges);
        drop(fork);
        allowance.release_peak(reserved);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn point_and_dense_occupied_entry_copy_across_subitem_pages() {
        let mut source = crate::constructive::tests::session();
        let cells = vec![Point { x: 0, y: 0 }, Point { x: 1, y: 0 }];
        source.routes[0] = Some(Route {
            connection: "connection-0".into(),
            source_port: None,
            target_port: None,
            cells: cells.clone(),
        });
        source.occupied[0].insert(cells[0], vec![0, 1, 2, 3]);
        let mut baseline_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut baseline_cursor = SessionForkCursor::new(&source);
        let ForkAdvance::Complete(_, Ok((baseline_fork, baseline_bytes))) =
            baseline_cursor.advance(&mut baseline_allowance, 0)
        else {
            panic!("unpaged baseline must complete");
        };
        let baseline_work = baseline_allowance.usage().categories;
        drop(baseline_fork);
        baseline_allowance.release_peak(baseline_bytes);
        for quota in [8, 64] {
            let budget = SearchMemoryBudget::new(1 << 20);
            let mut allowance = RequestAllowance::with_memory_budget(
                AllowanceCaps::default(),
                Some(budget.clone()),
            );
            let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
            let copied_index = CUMULATIVE_WORK_KINDS
                .iter()
                .position(|kind| *kind == WorkKind::CopiedBytes)
                .unwrap();
            quotas[copied_index] = quota;
            let mut cursor = SessionForkCursor::new(&source);
            let mut pages = 0;
            let (fork, reserved) = loop {
                assert!(pages < 1_000, "copy must progress with {quota}-byte pages");
                allowance.begin_page_window(quotas, Duration::from_secs(10));
                let result = cursor.advance(&mut allowance, 0);
                allowance.end_page_window();
                pages += 1;
                match result {
                    ForkAdvance::Pending(stop) => assert_eq!(stop.reason, "page-quantum"),
                    ForkAdvance::Complete(_, Ok(result)) => break result,
                    ForkAdvance::Complete(_, Err(failure)) => panic!("copy failed: {failure:?}"),
                }
            };
            assert!(pages > 1);
            assert_eq!(fork.routes, source.routes);
            assert_eq!(fork.occupied, source.occupied);
            assert_eq!(allowance.usage().categories, baseline_work);
            drop(fork);
            allowance.release_peak(reserved);
            assert_eq!(budget.live_bytes(), 0);
        }
    }

    #[test]
    fn impossible_aggregate_reservation_is_hard_and_returns_owned_source() {
        let original = populated_source();
        let original_id = original.instance_id.0;
        let budget = SearchMemoryBudget::new(1_024);
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        let mut cursor = SessionForkCursor::new_owned(original);
        let ForkAdvance::Complete(SourceOwner::Owned(returned), Err(RequestFailure::Stopped(stop))) =
            cursor.advance(&mut allowance, 0)
        else {
            panic!("impossible aggregate allocation must stop with its original source");
        };
        assert_eq!(stop.reason, "aggregate-memory-limit");
        assert_eq!(returned.instance_id.0, original_id);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn fork_copy_does_not_wait_for_its_pinned_source_to_release_memory() {
        let source = crate::constructive::tests::session();
        let budget = SearchMemoryBudget::new(1_024);
        budget.reserve(1_000).unwrap();
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        let mut cursor = SessionForkCursor::new(&source);
        let ForkAdvance::Complete(SourceOwner::Borrowed(_), Err(RequestFailure::Stopped(stop))) =
            cursor.advance(&mut allowance, 1_000)
        else {
            panic!("the source owner cannot release while its fork is pending");
        };
        assert_eq!(stop.reason, "aggregate-memory-limit");
        assert_eq!(budget.live_bytes(), 1_000);
        budget.release(1_000);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn aggregate_contention_retries_after_other_owner_releases_memory() {
        let source = crate::constructive::tests::session();
        let budget = SearchMemoryBudget::new(1 << 20);
        let other_bytes = (1 << 20) - 1_024;
        budget.reserve(other_bytes).unwrap();
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        let mut cursor = SessionForkCursor::new(&source);
        assert!(matches!(
            cursor.advance(&mut allowance, 0),
            ForkAdvance::Pending(stop) if stop.reason == "aggregate-memory-limit"
        ));
        budget.release(other_bytes);
        let ForkAdvance::Complete(_, Ok((fork, reserved))) = cursor.advance(&mut allowance, 0)
        else {
            panic!("copy must resume when competing memory is released");
        };
        assert_eq!(fork.routes, source.routes);
        drop(fork);
        allowance.release_peak(reserved);
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn hard_fork_memory_denial_releases_partial_copy() {
        let source = populated_source();
        let budget = SearchMemoryBudget::new(1 << 26);
        let mut caps = AllowanceCaps::default();
        caps.peak_live_bytes = 4_096;
        let mut allowance = RequestAllowance::with_memory_budget(caps, Some(budget.clone()));
        let mut cursor = SessionForkCursor::new(&source);
        assert!(matches!(
            cursor.advance(&mut allowance, 0),
            ForkAdvance::Complete(SourceOwner::Borrowed(_), Err(RequestFailure::Stopped(stop)))
                if stop.reason == "peak-memory-limit"
        ));
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn long_string_and_dense_users_make_progress_with_small_pages() {
        let mut source = populated_source();
        source.routes[0].as_mut().unwrap().connection = "a".repeat(400);
        source.occupied[0]
            .get_mut(&Point { x: 0, y: 0 })
            .unwrap()
            .extend(1..100);
        let budget = SearchMemoryBudget::new(1 << 26);
        let mut allowance =
            RequestAllowance::with_memory_budget(AllowanceCaps::default(), Some(budget.clone()));
        let mut quotas = [usize::MAX; CUMULATIVE_WORK_KINDS.len()];
        let copied_index = CUMULATIVE_WORK_KINDS
            .iter()
            .position(|kind| *kind == WorkKind::CopiedBytes)
            .unwrap();
        quotas[copied_index] = 128;
        let mut cursor = SessionForkCursor::new(&source);
        let mut pages = 0;
        let (fork, reserved) = loop {
            assert!(pages < 1_000, "copy cursor must make page progress");
            allowance.begin_page_window(quotas, Duration::from_secs(10));
            let result = cursor.advance(&mut allowance, 0);
            allowance.end_page_window();
            match result {
                ForkAdvance::Pending(stop) => {
                    assert_eq!(stop.reason, "page-quantum");
                    pages += 1;
                }
                ForkAdvance::Complete(SourceOwner::Borrowed(_), Ok(result)) => break result,
                ForkAdvance::Complete(_, Err(_)) => panic!("valid copy must complete"),
                ForkAdvance::Complete(_, Ok(_)) => panic!("source owner changed"),
            }
        };
        assert!(pages > 10);
        assert_eq!(fork.routes, source.routes);
        assert_eq!(fork.occupied, source.occupied);
        let paged_work = allowance.usage().categories;
        drop(fork);
        allowance.release_peak(reserved);
        assert_eq!(budget.live_bytes(), 0);

        let mut baseline_allowance = RequestAllowance::new(AllowanceCaps::default());
        let mut baseline = SessionForkCursor::new(&source);
        let ForkAdvance::Complete(_, Ok((baseline_fork, baseline_bytes))) =
            baseline.advance(&mut baseline_allowance, 0)
        else {
            panic!("unpaged copy must complete");
        };
        assert_eq!(baseline_fork.routes, source.routes);
        assert_eq!(baseline_fork.occupied, source.occupied);
        assert_eq!(paged_work, baseline_allowance.usage().categories);
        drop(baseline_fork);
        baseline_allowance.release_peak(baseline_bytes);
    }
}
