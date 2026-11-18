# Research model, not a production patch

This standalone model passed its local tests. It has not been integrated into
Acrux. Passing these tests does not prove the suggested policy is safe.

Critical assumption: the target slot in this toy model is NOT proven to be
an authenticated upper bound on the underlying transaction blockhash lifetime.
Before integration, prove that binding, reject stale/future inputs consistently,
define a consensus migration, and compare replica state at every checkpoint.
Different pruning cadences may converge after a final prune but can produce
different intermediate serialized state hashes. Consensus pruning must use
one agreed schedule. Do not delete replay protection based on this toy model.

# PoC: deterministic tombstone pruning horizon

Standalone sketch of the suggested fix for the review finding
"[Low] ThresholdCache tombstones (decoded/expired IDs) grow without bound and
are serialized into every checkpoint, state save and follower snapshot".

No dependencies. Run:

```
cargo test
```

What it demonstrates:

- The exact pruning rule (`disposition`): an id is prunable once
  `slot > target_slot + BLOCKHASH_AGE_SLOTS`, because its blockhash can never
  execute again — so the anti-replay tombstone has done its job.
- The rule is shared across admit / prune / replay, keeping the sets
  deterministic for validators and followers.
- `replay_determinism`: two replicas pruning on different cadences converge to
  identical state.
- `memory_stays_bounded_under_spam`: the 1M-spam-tx growth vector from the
  finding is capped at ~`BLOCKHASH_AGE_SLOTS x throughput` retained entries
  instead of growing with total chain history.

Integration points into `node/crates/acrux-execution/src/threshold.rs` are
marked `TODO(integrate)` in `src/lib.rs`. The one hard requirement: whatever
horizon is chosen must be applied identically in `accept_at`, `expire`, and
the follower replay path, or the sets stay consensus-critical (see the issue
for why).
