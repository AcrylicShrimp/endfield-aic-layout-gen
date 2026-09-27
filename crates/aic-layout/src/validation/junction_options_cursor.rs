use super::{Catalogs, Result, SiteArmUse, error, within_capacity};
use crate::geometry::Point;
use crate::logistics::{LogisticsComponentKind, TransportKind};
use crate::projection::cardinal_index;
use crate::rate::Rate;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JunctionReservation {
    Granted,
    Retry,
    Denied,
}

pub enum JunctionAdvance {
    Pending,
    Retry,
    Denied,
    Complete(Result<Option<Vec<(String, i64)>>>),
}

enum Phase {
    Uses,
    Catalog,
    Done,
}

pub struct JunctionOptionsCursor {
    transport: TransportKind,
    point: Point,
    phase: Phase,
    use_index: usize,
    incoming: [Rate; 4],
    outgoing: [Rate; 4],
    external_incoming: Rate,
    external_outgoing: Rate,
    total_in: Rate,
    in_dirs: Vec<usize>,
    out_dirs: Vec<usize>,
    kind: Option<LogisticsComponentKind>,
    definition_index: usize,
    definition_visited: bool,
    rotation_index: usize,
    rotation_visited: bool,
    option_copy: Option<String>,
    option_copy_offset: usize,
    options: Vec<(String, i64)>,
    reserved_bytes: usize,
}

impl JunctionOptionsCursor {
    pub fn new(transport: TransportKind, point: Point) -> Self {
        Self {
            transport,
            point,
            phase: Phase::Uses,
            use_index: 0,
            incoming: [Rate::zero(); 4],
            outgoing: [Rate::zero(); 4],
            external_incoming: Rate::zero(),
            external_outgoing: Rate::zero(),
            total_in: Rate::zero(),
            in_dirs: Vec::new(),
            out_dirs: Vec::new(),
            kind: None,
            definition_index: 0,
            definition_visited: false,
            rotation_index: 0,
            rotation_visited: false,
            option_copy: None,
            option_copy_offset: 0,
            options: Vec::new(),
            reserved_bytes: 0,
        }
    }

    pub fn reserved_bytes(&self) -> usize {
        self.reserved_bytes
    }

    fn site(&self) -> String {
        format!("{},{}", self.point.x, self.point.y)
    }

    fn process_use(&mut self, uses: &[SiteArmUse<'_>]) -> Result<()> {
        let use_site = &uses[self.use_index];
        if use_site.commodity != uses[0].commodity
            || use_site.incoming.is_some() == use_site.external_incoming
            || use_site.outgoing.is_some() == use_site.external_outgoing
        {
            return Err(error(
                "topology",
                self.site(),
                "Mixed commodity or incomplete directed route arm",
            ));
        }
        if let Some(d) = use_site.incoming {
            self.incoming[d] = self.incoming[d].checked_add(use_site.rate).map_err(|_| {
                error(
                    "topology",
                    use_site.connection,
                    "Incoming arm rate overflow",
                )
            })?;
        } else {
            self.external_incoming =
                self.external_incoming
                    .checked_add(use_site.rate)
                    .map_err(|_| {
                        error(
                            "topology",
                            use_site.connection,
                            "External incoming rate overflow",
                        )
                    })?;
        }
        if let Some(d) = use_site.outgoing {
            self.outgoing[d] = self.outgoing[d].checked_add(use_site.rate).map_err(|_| {
                error(
                    "topology",
                    use_site.connection,
                    "Outgoing arm rate overflow",
                )
            })?;
        } else {
            self.external_outgoing =
                self.external_outgoing
                    .checked_add(use_site.rate)
                    .map_err(|_| {
                        error(
                            "topology",
                            use_site.connection,
                            "External outgoing rate overflow",
                        )
                    })?;
        }
        self.use_index += 1;
        Ok(())
    }

    fn finish_uses(&mut self, catalogs: &Catalogs) -> Result<Option<()>> {
        let capacity = catalogs.transports.capacity(self.transport);
        let mut total_in = self.external_incoming;
        let mut total_out = self.external_outgoing;
        for d in 0..4 {
            if !self.incoming[d].is_zero() && !self.outgoing[d].is_zero() {
                return Err(error(
                    "topology",
                    self.site(),
                    "A physical arm carries opposing directions",
                ));
            }
            total_in = total_in
                .checked_add(self.incoming[d])
                .map_err(|_| error("topology", "routes", "Incoming flow overflow"))?;
            total_out = total_out
                .checked_add(self.outgoing[d])
                .map_err(|_| error("topology", "routes", "Outgoing flow overflow"))?;
        }
        if total_in != total_out
            || !within_capacity(total_in, capacity.quantity, capacity.duration_ms)
        {
            return Err(error(
                "topology",
                self.site(),
                "Site flow is unbalanced or exceeds line capacity",
            ));
        }
        self.total_in = total_in;
        self.in_dirs = (0..4).filter(|&d| !self.incoming[d].is_zero()).collect();
        self.out_dirs = (0..4).filter(|&d| !self.outgoing[d].is_zero()).collect();
        let in_degree = self.in_dirs.len() + usize::from(!self.external_incoming.is_zero());
        let out_degree = self.out_dirs.len() + usize::from(!self.external_outgoing.is_zero());
        if in_degree <= 1 && out_degree <= 1 {
            return Ok(None);
        }
        self.kind = Some(match (in_degree, out_degree) {
            (1, 2..=3) => LogisticsComponentKind::Splitter,
            (2..=3, 1) => LogisticsComponentKind::Converger,
            _ => {
                return Err(error(
                    "topology",
                    self.site(),
                    "No catalog junction topology supports these directed arms",
                ));
            }
        });
        if !self.external_incoming.is_zero() || !self.external_outgoing.is_zero() {
            return Err(error(
                "topology",
                self.site(),
                "A boundary terminal cannot host a junction",
            ));
        }
        Ok(Some(()))
    }

    pub fn advance(
        &mut self,
        uses: &[SiteArmUse<'_>],
        catalogs: &Catalogs,
        mut visit_user: impl FnMut() -> bool,
        mut visit_catalog: impl FnMut() -> bool,
        mut copy_option: impl FnMut(usize) -> usize,
        mut reserve_option: impl FnMut(usize) -> JunctionReservation,
        mut release_option: impl FnMut(usize),
    ) -> JunctionAdvance {
        if uses.is_empty() {
            return JunctionAdvance::Complete(Err(error(
                "topology",
                self.site(),
                "Empty transport site",
            )));
        }
        if matches!(self.phase, Phase::Uses) {
            while self.use_index < uses.len() {
                if !visit_user() {
                    return JunctionAdvance::Pending;
                }
                if let Err(failure) = self.process_use(uses) {
                    return JunctionAdvance::Complete(Err(failure));
                }
            }
            match self.finish_uses(catalogs) {
                Ok(None) => return JunctionAdvance::Complete(Ok(None)),
                Ok(Some(())) => self.phase = Phase::Catalog,
                Err(failure) => return JunctionAdvance::Complete(Err(failure)),
            }
        }
        let definitions = &catalogs.components.catalog().components;
        while self.definition_index < definitions.len() {
            let definition = &definitions[self.definition_index];
            if !self.definition_visited {
                if !visit_catalog() {
                    return JunctionAdvance::Pending;
                }
                self.definition_visited = true;
                if definition.transport != self.transport
                    || Some(definition.kind) != self.kind
                    || definition.footprint.width != 1
                    || definition.footprint.height != 1
                    || !within_capacity(
                        self.total_in,
                        definition.capacity.quantity,
                        definition.capacity.duration_ms,
                    )
                {
                    self.definition_index += 1;
                    self.definition_visited = false;
                    continue;
                }
            }
            while let Some(&rotation) = definition.allowed_rotations.get(self.rotation_index) {
                if !self.rotation_visited {
                    if !visit_catalog() {
                        return JunctionAdvance::Pending;
                    }
                    self.rotation_visited = true;
                }
                let rotated = |direction: &crate::logistics::CardinalDirection| {
                    (cardinal_index(*direction) + rotation as usize / 90) % 4
                };
                let valid_in = self.in_dirs.iter().all(|&d| {
                    definition
                        .input_directions
                        .iter()
                        .any(|arm| rotated(arm) == d)
                });
                let valid_out = self.out_dirs.iter().all(|&d| {
                    definition
                        .output_directions
                        .iter()
                        .any(|arm| rotated(arm) == d)
                });
                if valid_in && valid_out {
                    if self.option_copy.is_none() {
                        let growth = self.options.len() == self.options.capacity();
                        let old_bytes = if growth {
                            self.options
                                .capacity()
                                .saturating_mul(std::mem::size_of::<(String, i64)>())
                        } else {
                            0
                        };
                        let new_bytes = if growth {
                            self.options
                                .capacity()
                                .saturating_mul(2)
                                .max(4)
                                .saturating_mul(std::mem::size_of::<(String, i64)>())
                        } else {
                            0
                        };
                        let bytes = new_bytes.saturating_add(definition.id.len());
                        match reserve_option(bytes) {
                            JunctionReservation::Granted => {}
                            JunctionReservation::Retry => return JunctionAdvance::Retry,
                            JunctionReservation::Denied => return JunctionAdvance::Denied,
                        }
                        if growth {
                            self.options.reserve(1);
                        }
                        self.option_copy = Some(String::with_capacity(definition.id.len()));
                        self.reserved_bytes = self.reserved_bytes.saturating_add(bytes);
                        if old_bytes > 0 {
                            self.reserved_bytes -= old_bytes;
                            release_option(old_bytes);
                        }
                    }
                    while self.option_copy_offset < definition.id.len() {
                        let offered = (definition.id.len() - self.option_copy_offset).min(64);
                        let granted = copy_option(offered);
                        if granted == 0 {
                            return JunctionAdvance::Pending;
                        }
                        debug_assert!(granted <= offered, "copy grant exceeds offered prefix");
                        let end = self.option_copy_offset + granted;
                        // Validated component IDs are ASCII stable IDs.
                        let chunk = &definition.id[self.option_copy_offset..end];
                        self.option_copy
                            .as_mut()
                            .expect("option copy was reserved")
                            .push_str(chunk);
                        self.option_copy_offset = end;
                    }
                    self.options.push((
                        self.option_copy.take().expect("complete option copy"),
                        rotation,
                    ));
                    self.option_copy_offset = 0;
                }
                self.rotation_index += 1;
                self.rotation_visited = false;
            }
            self.definition_index += 1;
            self.definition_visited = false;
            self.rotation_index = 0;
            self.rotation_visited = false;
        }
        self.phase = Phase::Done;
        if self.options.is_empty() {
            JunctionAdvance::Complete(Err(error(
                "junction",
                self.site(),
                "No catalog junction supports the directed arms, flow and rotation",
            )))
        } else {
            JunctionAdvance::Complete(Ok(Some(std::mem::take(&mut self.options))))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::facilities::{FacilityCatalog, FacilityFootprint, ValidatedFacilityCatalog};
    use crate::logistics::{
        CardinalDirection, LogisticsComponentCatalog, LogisticsComponentDefinition,
        TransportCapacity, TransportCatalog, TransportDefinition,
        ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    };
    use crate::validation::junction_options_with_visit;

    fn catalogs() -> Catalogs {
        let capacity = TransportCapacity {
            quantity: 1,
            duration_ms: 1000,
        };
        let mut components = Vec::new();
        for transport in [TransportKind::Belt, TransportKind::Pipe] {
            for kind in [
                LogisticsComponentKind::Splitter,
                LogisticsComponentKind::Converger,
                LogisticsComponentKind::Bridge,
            ] {
                let (input_directions, output_directions) = match kind {
                    LogisticsComponentKind::Splitter => (
                        vec![CardinalDirection::North],
                        vec![
                            CardinalDirection::East,
                            CardinalDirection::South,
                            CardinalDirection::West,
                        ],
                    ),
                    LogisticsComponentKind::Converger => (
                        vec![
                            CardinalDirection::North,
                            CardinalDirection::East,
                            CardinalDirection::West,
                        ],
                        vec![CardinalDirection::South],
                    ),
                    LogisticsComponentKind::Bridge => (
                        vec![
                            CardinalDirection::North,
                            CardinalDirection::East,
                            CardinalDirection::South,
                            CardinalDirection::West,
                        ],
                        vec![
                            CardinalDirection::North,
                            CardinalDirection::East,
                            CardinalDirection::South,
                            CardinalDirection::West,
                        ],
                    ),
                };
                components.push(LogisticsComponentDefinition {
                    id: format!("{transport:?}-{kind:?}").to_lowercase(),
                    transport,
                    kind,
                    footprint: FacilityFootprint {
                        width: 1,
                        height: 1,
                    },
                    allowed_rotations: vec![0, 90, 180, 270],
                    input_directions,
                    output_directions,
                    capacity: capacity.clone(),
                });
            }
        }
        Catalogs {
            identity: "f".repeat(64),
            facilities: ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
                schema_version: 3,
                facilities: Vec::new(),
            })
            .unwrap(),
            transports: ValidatedTransportCatalog::try_from_catalog(TransportCatalog {
                schema_version: 1,
                transports: [TransportKind::Belt, TransportKind::Pipe]
                    .into_iter()
                    .map(|kind| TransportDefinition {
                        kind,
                        capacity: capacity.clone(),
                    })
                    .collect(),
            })
            .unwrap(),
            components: ValidatedLogisticsComponentCatalog::try_from_catalog(
                LogisticsComponentCatalog {
                    schema_version: 1,
                    components,
                },
            )
            .unwrap(),
        }
    }

    #[test]
    fn catalog_retry_resumes_after_visited_rotation() {
        let catalogs = catalogs();
        let rate = Rate {
            numerator: 1,
            denominator: 2,
        };
        let uses = [
            SiteArmUse {
                connection: "a",
                commodity: "ore",
                rate,
                incoming: Some(3),
                outgoing: Some(0),
                external_incoming: false,
                external_outgoing: false,
            },
            SiteArmUse {
                connection: "b",
                commodity: "ore",
                rate,
                incoming: Some(3),
                outgoing: Some(1),
                external_incoming: false,
                external_outgoing: false,
            },
        ];
        let point = Point { x: 10, y: 20 };
        let mut uninterrupted_catalog_visits = 0;
        let expected =
            junction_options_with_visit(TransportKind::Belt, point, &uses, &catalogs, || {
                uninterrupted_catalog_visits += 1;
                true
            })
            .unwrap();
        assert_eq!(expected, Some(vec![("belt-splitter".into(), 0)]));
        let mut cursor = JunctionOptionsCursor::new(TransportKind::Belt, point);
        let mut catalog_visits = 0;
        let mut user_visits = 0;
        let mut retry_once = true;
        let actual = loop {
            let credit = Cell::new(1);
            let visit_user = || {
                if credit.get() == 0 {
                    false
                } else {
                    credit.set(0);
                    user_visits += 1;
                    true
                }
            };
            let visit_catalog = || {
                if credit.get() == 0 {
                    false
                } else {
                    credit.set(0);
                    catalog_visits += 1;
                    true
                }
            };
            match cursor.advance(
                &uses,
                &catalogs,
                visit_user,
                visit_catalog,
                |bytes| bytes,
                |_| {
                    if retry_once {
                        retry_once = false;
                        JunctionReservation::Retry
                    } else {
                        JunctionReservation::Granted
                    }
                },
                |_| {},
            ) {
                JunctionAdvance::Pending | JunctionAdvance::Retry => {}
                JunctionAdvance::Complete(result) => break result.unwrap(),
                JunctionAdvance::Denied => panic!("test reservation cannot deny"),
            }
        };
        assert_eq!(actual, expected);
        assert_eq!(user_visits, uses.len());
        assert_eq!(catalog_visits, uninterrupted_catalog_visits);
    }
}
