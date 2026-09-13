# Bounded declarative replay retention

Status: implemented and locally verified; enabled only by an explicit retention
configuration. The server and long-run launch remain on user hold.

The user asked to replace the least needed successful experience when the buffer
fills. This supersedes the short screen's fail-at-capacity behavior for the new
long configuration; it does not change frozen comparison runs. Keep 4,096 unique
active sequences and 262,144 label references. Sampling within the retained pool
continues to use the existing balanced-available rule.

Use the stage-balanced mean of `-log_probability * max(return - value, 0)` from
actual SIL draws as an observed-contribution proxy. This is already computed by
the optimizer. It is not proof of causal importance, and a zero caused by a critic
estimate does not establish that a trajectory is permanently useless. Record a
per-update EMA (alpha 0.5), observed draw/update counts and last observed update.
Require eight draws in at least two updates before treating the proxy as mature.
Discount stale utility by `1 + age_in_updates / 64` when comparing candidates.

When admitting a new unique success would exceed either capacity, count it in
its product stratum and choose the most represented existing stratum for removal;
a tie prefers the incoming stratum. This lets scarce large-factory experience
grow toward equal representation instead of replacing its own rare examples.
Within that stratum, remove the mature entry with the lowest discounted proxy.
Break ties by older admission and then stable identity. If none is mature, remove
the oldest entry and explicitly report insufficient observations as the fallback.
New/unobserved entries are protected preferentially, not absolutely: capacity
remains a hard bound. Apply replacement only at true episode-terminal admission.
An individually unrepresentable sequence is rejected before any eviction.

Persist utility in checkpointed entry metadata. Remove evicted decoded rows and
per-entry use counters; retain cumulative use/admission/eviction counters.
Rediscovered evicted experience may be admitted again, so cumulative admissions
are not the current pool's unique-entry count. Duplicate handling for active
entries and full terminal credit remain unchanged. Publication retention keeps
any old source chunk still required by a selected/recovery checkpoint or live
pending episode, even after its active replay entry was evicted.

The measurement shares the existing device-to-host diagnostic transfer and adds
no forward/backward pass or whole-buffer inference. Local release-native tests
confirm identical model, both Adam states, shuffle RNG and forward counts with
measurement enabled or disabled on a nonempty successful replay update. Other
checks cover stage balancing, scarce-stratum protection, immature-entry fallback,
staleness, restoration, actual capacity replacement and cache/source references.
Actual server/CUDA continuation readiness remains a separate pre-launch gate.

## Withdrawn experimental evaluation, 2026-09-14

The following historical outcome and baseline recommendation were withdrawn at
the user's request. They do not establish a best setup and must not gate the
[replacement quality study](2026-09-14-quality-weighted-replay-study.md).

### Historical record

The opt-in `declarative-replay-category-v2` policy and explicit uniform,
score-mixture and positive-advantage sampling are implemented and locally/native
verified. They remain experimental and are not enabled on the main launch.
The [registered chronological screen](../reports/2026-09-14-chronological-retention-screen.md)
completed 25 profiles times three seeds with no shortlisted candidate. Category
protection improved retention duration, but no profile met the registered adoption
gates. No candidate learning or runtime superiority was established. The
[final decision](../reports/2026-09-14-training-setup-verdict.md) keeps v1 with
balanced uniform eight-sequence replay and leaves main training paused at update
1744. Existing default numerical behavior remains verified unchanged.
