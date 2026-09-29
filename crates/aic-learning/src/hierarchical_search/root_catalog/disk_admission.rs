//! One boundary check for a root's simultaneous temporary-file peak.

use std::{io, path::Path};

use super::{CatalogStop, LEDGER_ROW_BYTES, RUN_ROWS, sort_row_bytes};
use crate::hierarchical_search::root_scoring::RootNoiseMode;

const DIRECTORY_HEADROOM_BYTES: u64 = 64 * 1024;
const RUN_FILE_HEADROOM_BYTES: u64 = 8 * 1024;

/// The filesystem's available space already excludes sidecars held by the
/// active replay window. `current_raw_bytes` is subtracted because those bytes
/// are also already excluded from the available-space reading.
pub(super) fn admit_peak(
    prefix: &Path,
    mode: RootNoiseMode,
    actions: u64,
    current_raw_bytes: u64,
    run_rows: usize,
    page_size: usize,
) -> Result<(), CatalogStop> {
    if actions == 0 {
        return Ok(());
    }
    let directory = prefix
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let available = fs4::available_space(directory).map_err(|source| CatalogStop::Io {
        path: directory.to_owned(),
        operation: "measure available space",
        source,
    })?;
    let needed = additional_bytes(mode, actions, current_raw_bytes, run_rows, page_size);
    admit_available(directory, mode, actions, needed, available)
}

fn admit_available(
    directory: &Path,
    mode: RootNoiseMode,
    actions: u64,
    needed: u64,
    available: u64,
) -> Result<(), CatalogStop> {
    if available < needed {
        return Err(CatalogStop::Io {
            path: directory.to_owned(),
            operation: "admit disk peak",
            source: io::Error::new(
                io::ErrorKind::StorageFull,
                format!(
                    "root needs {needed} more bytes for {actions} actions in {mode:?} mode; {available} bytes available including the retained replay window"
                ),
            ),
        });
    }
    Ok(())
}

pub(super) fn planned_run_rows(available_tree_bytes: usize, row_bytes: usize) -> usize {
    RUN_ROWS
        .min(available_tree_bytes.saturating_sub(8192) / (4 * row_bytes))
        .max(1)
}

fn additional_bytes(
    mode: RootNoiseMode,
    actions: u64,
    current_raw_bytes: u64,
    run_rows: usize,
    page_size: usize,
) -> u64 {
    if actions == 0 {
        return 0;
    }
    let raw = mode.record_bytes() as u64;
    let sorted = sort_row_bytes(mode) as u64;
    let ledger = LEDGER_ROW_BYTES as u64;
    let normalization = raw.saturating_add(sorted).saturating_add(ledger);
    let run_count = run_count(actions, run_rows, page_size);
    let merging = if run_count > 1 {
        sorted.saturating_mul(2).saturating_add(ledger)
    } else {
        0
    };
    let headroom = DIRECTORY_HEADROOM_BYTES.saturating_add(
        run_count
            .saturating_mul(2)
            .saturating_add(2)
            .saturating_mul(RUN_FILE_HEADROOM_BYTES),
    );
    actions
        .saturating_mul(normalization.max(merging))
        .saturating_add(headroom)
        .saturating_sub(current_raw_bytes)
}

fn run_count(actions: u64, run_rows: usize, page_size: usize) -> u64 {
    if actions == 0 {
        return 0;
    }
    let rows = run_rows.max(1) as u64;
    let page = page_size.max(1).min(run_rows.max(1)) as u64;
    let full_pages = actions / page;
    let tail = actions % page;
    let pages_per_run = rows / page;
    let mut runs = full_pages.div_ceil(pages_per_run);
    if tail > 0 {
        let filled_last_run = if full_pages == 0 {
            0
        } else {
            ((full_pages - 1) % pages_per_run + 1) * page
        };
        if full_pages == 0 || filled_last_run + tail > rows {
            runs += 1;
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_peak_counts_only_simultaneous_files_and_existing_raw() {
        assert_eq!(
            additional_bytes(RootNoiseMode::Rank, 0, 0, RUN_ROWS, 256),
            0
        );
        let n = 1_000_000;
        let overhead = DIRECTORY_HEADROOM_BYTES + 4 * RUN_FILE_HEADROOM_BYTES;
        assert_eq!(
            additional_bytes(RootNoiseMode::Rank, n, 0, RUN_ROWS, 256),
            n * 52 + overhead
        );
        assert_eq!(
            additional_bytes(RootNoiseMode::ActionIdentity, n, n * 20, RUN_ROWS, 256),
            n * (68 - 20) + overhead
        );
        assert_eq!(
            additional_bytes(RootNoiseMode::Rank, n, 0, RUN_ROWS / 2, 256),
            n * 64 + DIRECTORY_HEADROOM_BYTES + 6 * RUN_FILE_HEADROOM_BYTES
        );
        assert_eq!(
            additional_bytes(RootNoiseMode::Rank, 900, 0, 1_000, 256),
            900 * 52 + DIRECTORY_HEADROOM_BYTES + 4 * RUN_FILE_HEADROOM_BYTES
        );
        assert_eq!(run_count(1_000, 1_000, 256), 1);
        assert_eq!(run_count(1_001, 1_000, 256), 2);
    }

    #[test]
    fn insufficient_space_is_a_typed_retryable_disk_stop() {
        let needed = additional_bytes(RootNoiseMode::ActionIdentity, 100, 0, RUN_ROWS, 256);
        let stopped = admit_available(
            Path::new("/tmp"),
            RootNoiseMode::ActionIdentity,
            100,
            needed,
            needed - 1,
        );
        assert!(
            matches!(stopped, Err(CatalogStop::Io { source, operation: "admit disk peak", .. }) if source.kind() == io::ErrorKind::StorageFull)
        );
        assert!(
            admit_available(
                Path::new("/tmp"),
                RootNoiseMode::ActionIdentity,
                100,
                needed,
                needed
            )
            .is_ok()
        );
    }
}
