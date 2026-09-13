# Four-hour continuation preparation

Status: local preparation complete; server verification and launch remain on user hold.

Context: one seed-29, batch-1,024 continuation for four additional hours. Parent
is update 32 of the completed batch comparison. Preserve all established learning
and input contracts; resolve the v2 import and short-screen replay ceiling.

Completion criteria:
- Prepare the duration/configuration and exact v2 import with durable recovery.
- Replace least useful replay entries at capacity, preserving size-stratum coverage.
- Verify locally and commit the prepared source/configuration.
- Document the exact server verification/start commands and keep them unexecuted.
- Close actual native/CUDA/performance continuity only after the hold is lifted.

Activity:
- Inspected parent provenance, native/pool, importer, replay, duration, retention,
  tracking and reporting contracts; found the v1-only import and 4,096-entry cap.
- Server has not been contacted since the preparation-only instruction.
- Implemented explicit v2 import, complete-wave duration/signal stopping, pause
  evaluation restoration, best-model retention and dynamic final reporting.
- Implemented user-requested fixed-capacity replay replacement from already
  measured SIL contribution, with scarce-stratum protection and recorded fallback.
- Passed 27 targeted local tests with the release-native library, workspace
  check/tests and the non-executing configuration CLI. Utility measurement adds
  no forward calls and preserves the optimizer transition in the local proof.
- Documented the exact isolated server readiness, preparation and start commands.
  No server connection, synchronization, verification, training or schedule was
  performed after the preparation-only instruction. Server readiness is pending.
