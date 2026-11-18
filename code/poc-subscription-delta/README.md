# Research model, not a production patch

This standalone model passed its local tests. It has not been integrated into
Acrux. Passing these tests does not prove the suggested policy is safe.

The model uses a per-CONNECTION bucket. It does not implement the shared
per-IP budget used by Acrux HTTP reads. Share that real budget across sockets
in production. Charging one token for 64 reads does not bound per-account work.
Denied changes are discarded by this toy; production must retain/coalesce
pending updates or return an explicit gap/resync signal. There is no async
actor, slot deduplication, byte bound, or production notification contract here.

# PoC: change-detected, coalesced accountSubscribe notifications

Standalone sketch of the suggested fix for the review finding
"[Low] accountSubscribe notifications bypass the read rate limiter and
multiply node-actor commands by 64 per connection per slot".

No dependencies. Run:

```
cargo test
```

What it demonstrates:

- Today: every subscribed account is re-fetched every slot, one actor command
  each, with `client: None` (outside the per-IP read bucket): up to
  8 conns x 64 subs x 20 slots/s = 10,240 actor commands/s per IP.
- This model: slots report their write set; a connection only causes work for
  accounts that actually changed; all of a connection's matches coalesce into
  ONE actor command per slot; and that read is charged to the connection's
  real rate-limit bucket.
- `amplification_collapses_to_one_command_per_connection`: 8x64 subscriptions,
  one dirty account -> 8 commands, not 512.
- `reads_are_charged_to_the_client_bucket`: after the 50-read window is
  drained, slots produce zero queued work (a flood cannot queue).

Integration points into `node/crates/acrux-node/src/gateway.rs` are marked
`TODO(integrate)` in `src/lib.rs`. The one prerequisite is plumbing the
executed slot's write set into the notification path (change detection), as
Solana's own PubSub does.
