//! RESEARCH MODEL ONLY: buckets are per connection, not shared per IP.
//! Denied changes are dropped here; read README for required production behavior.
//! PoC: change-detected, coalesced account notifications for accountSubscribe.
//!
//! Sketch of the fix suggested in the review finding:
//! "[Low] accountSubscribe notifications bypass the read rate limiter and
//! multiply node-actor commands by 64 per connection per slot".
//!
//! Current behavior (node/crates/acrux-node/src/gateway.rs): every
//! accountSubscribe'd account is re-fetched on every slot.executed, each fetch
//! an actor command with `client: None` (outside the per-IP read bucket).
//!
//! Fix modeled here:
//!   1. Slots report which accounts they actually dirtied; only dirty accounts
//!      that a connection subscribes to cause work (Solana PubSub semantics).
//!   2. Per connection per slot, all matching subscriptions coalesce into ONE
//!      actor command (batched getMultipleAccounts-style read).
//!   3. That read is charged against the connection's real rate-limit bucket.
//!
//! Standalone (no acrux deps); pure logic with tests proving the amplification
//! collapse. Integration points marked `TODO(integrate)`.

use std::collections::{BTreeMap, BTreeSet};

pub type ConnId = u64;
pub type AccountId = String;

#[derive(Default, Debug)]
pub struct DeltaNotifier {
    /// conn -> subscribed accounts.
    subs: BTreeMap<ConnId, BTreeSet<AccountId>>,
    /// conn -> remaining read tokens this window (the real client bucket).
    buckets: BTreeMap<ConnId, u32>,
    pub commands_issued: u64,
    pub reads_charged: u64,
    pub reads_denied: u64,
}

pub const MAX_SUBS_PER_CONN: usize = 64;
const BUCKET_CAPACITY: u32 = 50; // mirrors ACRUX_RPC_RATE per window

impl DeltaNotifier {
    /// TODO(integrate): replaces the unbounded `SolanaSub::Account` insert in
    /// gateway.rs; keep the existing MAX_SOLANA_SUBS = 64 bound.
    pub fn subscribe(&mut self, conn: ConnId, account: AccountId) -> Result<(), &'static str> {
        let set = self.subs.entry(conn).or_default();
        if set.len() >= MAX_SUBS_PER_CONN && !set.contains(&account) {
            return Err("subscription limit");
        }
        set.insert(account);
        self.buckets.entry(conn).or_insert(BUCKET_CAPACITY);
        Ok(())
    }

    pub fn unsubscribe_all(&mut self, conn: ConnId) {
        self.subs.remove(&conn);
        self.buckets.remove(&conn);
    }

    /// Called once per executed slot with the accounts that slot modified.
    /// Returns the (conn, accounts) batches to fetch: at most one per
    /// connection, only for dirty-and-subscribed accounts, only while the
    /// client's read bucket has tokens.
    ///
    /// TODO(integrate): the executed-slot event must carry the slot's write
    /// set; gateway.rs currently has no change detection. The returned batch
    /// becomes ONE GatewayCommand with the REAL client identity, not None.
    pub fn on_slot_executed(&mut self, dirty: &BTreeSet<AccountId>) -> Vec<(ConnId, Vec<AccountId>)> {
        let mut out = Vec::new();
        for (conn, set) in &self.subs {
            let hits: Vec<AccountId> = set.intersection(dirty).cloned().collect();
            if hits.is_empty() {
                continue; // nothing changed for this connection: zero actor commands
            }
            let bucket = self.buckets.entry(*conn).or_insert(BUCKET_CAPACITY);
            if *bucket == 0 {
                self.reads_denied += 1;
                continue; // over budget: skipped, not queued (a flood cannot queue work)
            }
            *bucket -= 1;
            self.reads_charged += 1;
            self.commands_issued += 1;
            out.push((*conn, hits));
        }
        out
    }

    /// Refill buckets once per rate-limit window.
    pub fn refill_window(&mut self) {
        for b in self.buckets.values_mut() {
            *b = BUCKET_CAPACITY;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn_with_n_subs(notifier: &mut DeltaNotifier, conn: ConnId, n: usize) {
        for i in 0..n {
            notifier.subscribe(conn, format!("acct-{i}")).unwrap();
        }
    }

    #[test]
    fn amplification_collapses_to_one_command_per_connection() {
        // The finding's worst case: 8 connections x 64 subscriptions, 20 slots/s
        // -> 10,240 actor commands/s per IP today. With change detection and
        // coalescing, a slot that dirties ONE subscribed account costs at most
        // ONE command per connection.
        let mut n = DeltaNotifier::default();
        for conn in 0..8 {
            conn_with_n_subs(&mut n, conn, 64);
        }
        let mut dirty = BTreeSet::new();
        dirty.insert("acct-0".to_string());
        let batches = n.on_slot_executed(&dirty);
        assert_eq!(batches.len(), 8); // one per connection, not 64 per connection
        assert_eq!(n.commands_issued, 8);
        // And a slot that dirties nothing subscribed costs zero.
        let dirty_none: BTreeSet<AccountId> = ["unsubscribed".to_string()].into_iter().collect();
        assert!(n.on_slot_executed(&dirty_none).is_empty());
        assert_eq!(n.commands_issued, 8);
    }

    #[test]
    fn reads_are_charged_to_the_client_bucket() {
        let mut n = DeltaNotifier::default();
        conn_with_n_subs(&mut n, 1, 64);
        let dirty: BTreeSet<AccountId> = (0..64).map(|i| format!("acct-{i}")).collect();
        // Drain the 50-token window: after that, slots produce no work at all.
        for _ in 0..50 {
            assert_eq!(n.on_slot_executed(&dirty).len(), 1);
        }
        assert!(n.on_slot_executed(&dirty).is_empty());
        assert_eq!(n.reads_charged, 50);
        assert_eq!(n.reads_denied, 1);
        n.refill_window();
        assert_eq!(n.on_slot_executed(&dirty).len(), 1);
    }

    #[test]
    fn unsubscribe_stops_all_work() {
        let mut n = DeltaNotifier::default();
        conn_with_n_subs(&mut n, 7, 10);
        n.unsubscribe_all(7);
        let dirty: BTreeSet<AccountId> = (0..10).map(|i| format!("acct-{i}")).collect();
        assert!(n.on_slot_executed(&dirty).is_empty());
    }
}
