//! Shared exact crossing preparation for one immutable route request.

mod cursor;
pub(super) use cursor::RouteCrossingCursor;

use aic_routing::Crossings;

use super::{Config, RequestAllowance, Session};

pub(super) fn route_crossings(
    session: &Session,
    connection: usize,
    config: &Config,
    allowance: &mut RequestAllowance,
) -> Result<Crossings, super::AllowanceStop> {
    let mut cursor = RouteCrossingCursor::new(session, config);
    cursor.advance(session, connection, config, allowance)?;
    Ok(cursor.finish())
}
