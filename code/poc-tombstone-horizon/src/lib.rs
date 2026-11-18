//! RESEARCH MODEL ONLY: target_slot is assumed to bound validity here.
//! This assumption is NOT proven for Acrux; read README before reuse.
//! PoC: deterministic pruning horizon for threshold tombstones.
//!
//! Sketch of the fix suggested in the review finding:
//! "[Low] ThresholdCache tombstones (decoded/expired IDs) grow without bound..."
//!
//! Idea: a tombstone for transaction id X only matters while a re-included
//! fragment of X could still execute. Once `slot > target_slot + BLOCKHASH_AGE_SLOTS`,
//! X's blockhash is expired, so a re-included fragment can never execute and the
//! tombstone is dead weight. The same horizon must be applied in THREE places or
//! the sets stay consensus-critical:
//!   1. `accept_at` / inclusion: fragments past the horizon are inadmissible
//!      (reject before charging inclusion),
//!   2. `expire`: drop (not keep) entries past the horizon,
//!   3. follower replay: apply the identical rule, byte for byte.
//!
//! This file models that rule standalone (no acrux dependencies) so the
//! semantics and edge cases can be agreed on before touching `threshold.rs`.
//! Integration points in the real code are marked `TODO(integrate)`.

/// Slots a recent blockhash stays usable for. Mirrors the chain constant.
pub const BLOCKHASH_AGE_SLOTS: u64 = 1200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// Normal processing; the id may be pending, decoded, or a live tombstone.
    Live,
    /// Past the horizon: never executable again. Inadmissible before charging,
    /// and its tombstone may be dropped.
    Prunable,
}

/// The single rule every code path must share.
pub fn disposition(slot: u64, target_slot: u64) -> Disposition {
    if slot > target_slot.saturating_add(BLOCKHASH_AGE_SLOTS) {
        Disposition::Prunable
    } else {
        Disposition::Live
    }
}

#[derive(Default, Debug)]
pub struct HorizonTombstones {
    /// id -> target_slot recorded when the id was first seen.
    seen: std::collections::BTreeMap<String, u64>,
    pub pruned_total: u64,
}

impl HorizonTombstones {
    /// Record an id. Returns false if the id is already past the horizon at
    /// `slot` (inadmissible: reject before charging any inclusion fee).
    ///
    /// TODO(integrate): call from `ThresholdCache::accept_at` / `reject` /
    /// `expire` entry paths, replacing unconditional insertion.
    pub fn admit(&mut self, slot: u64, id: &str, target_slot: u64) -> bool {
        if disposition(slot, target_slot) == Disposition::Prunable {
            return false;
        }
        self.seen.entry(id.to_string()).or_insert(target_slot);
        true
    }

    /// Whether a re-included fragment for `id` must meet its tombstone outcome
    /// at `slot`. Past the horizon the answer is deterministically "no".
    pub fn is_live(&self, slot: u64, id: &str) -> bool {
        match self.seen.get(id) {
            Some(&t) => disposition(slot, t) == Disposition::Live,
            None => false,
        }
    }

    /// Drop everything past the horizon. Deterministic: depends only on
    /// `slot`, which all validators and followers agree on.
    ///
    /// TODO(integrate): call from `ThresholdCache::expire`, and make sure the
    /// follower replay path replays the SAME calls so serialized state stays
    /// identical everywhere.
    pub fn prune(&mut self, slot: u64) {
        let before = self.seen.len();
        self.seen.retain(|_, t| disposition(slot, *t) == Disposition::Live);
        self.pruned_total += (before - self.seen.len()) as u64;
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_survive_within_horizon_and_prune_after() {
        let mut t = HorizonTombstones::default();
        assert!(t.admit(8, "a", 8));
        t.prune(8 + BLOCKHASH_AGE_SLOTS); // boundary: still live
        assert_eq!(t.len(), 1);
        assert!(t.is_live(8 + BLOCKHASH_AGE_SLOTS, "a"));
        t.prune(8 + BLOCKHASH_AGE_SLOTS + 1); // one slot past: gone
        assert_eq!(t.len(), 0);
        assert!(!t.is_live(8 + BLOCKHASH_AGE_SLOTS + 1, "a"));
    }

    #[test]
    fn past_horizon_ids_are_inadmissible() {
        let mut t = HorizonTombstones::default();
        // A fragment whose target_slot is ancient must be rejected before charging.
        assert!(!t.admit(10_000, "old", 8));
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn replay_determinism() {
        // Two replicas applying the same op stream, pruning at different
        // frequencies, must converge to identical live state at the same slot.
        let ops: Vec<(u64, String, u64)> = (0..10_000u64)
            .map(|i| (8 + i, format!("tx-{i:05}"), 8 + i))
            .collect();
        let mut a = HorizonTombstones::default();
        let mut b = HorizonTombstones::default();
        for (slot, id, target) in &ops {
            a.admit(*slot, id, *target);
            b.admit(*slot, id, *target);
            if slot % 7 == 0 {
                a.prune(*slot);
            }
            if slot % 13 == 0 {
                b.prune(*slot);
            }
        }
        let final_slot = 8 + 10_000 + BLOCKHASH_AGE_SLOTS + 1;
        a.prune(final_slot);
        b.prune(final_slot);
        assert_eq!(a.seen, b.seen, "prune cadence must not change final state");
    }

    #[test]
    fn memory_stays_bounded_under_spam() {
        // The attack from the finding: one included fragment per unique id,
        // left to expire. With the horizon, retained state is bounded by the
        // horizon window, not by chain history.
        let mut t = HorizonTombstones::default();
        let mut slot = 8u64;
        for i in 0..1_000_000u64 {
            t.admit(slot, &format!("spam-{i}"), slot);
            if i % 10 == 0 {
                slot += 1; // 10 spam txs per slot
                t.prune(slot);
            }
        }
        // ~1200 live slots x 10 ids/slot, not 1M.
        assert!(t.len() <= (BLOCKHASH_AGE_SLOTS as usize + 2) * 10);
        assert_eq!(t.pruned_total + t.len() as u64, 1_000_000);
    }
}
