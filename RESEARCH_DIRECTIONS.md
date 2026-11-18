# Acrux: research directions and developer experiments

Research snapshot: 18 November 2026. Repository checked locally at `7703b91ac3a2f872667d1f958f63f4a81b4ad151` (node 0.4.12 / contract 0.1.18). This is a research roadmap, not an audit certificate, deployment recommendation, or claim of exhaustive coverage of the latest literature.

## Recommended order

| Priority | Direction | First useful result | Acceptance criterion |
| --- | --- | --- | --- |
| P0 | Verification and regression testing | Anchored follower tests and bridge handoff/restart tests | Forged certificates, stale epochs, and duplicate withdrawals rejected |
| P1 | Bounded resource use | Subscription batching and replay-safe retention design | Bounded queues/state, preserved notification and replay semantics |
| P2 | Parallel execution measurement | Hot-account benchmark with serial reference | Identical state hash and measured p99 slot latency |
| P2 | Perpetuals compatibility lab | Local-only percolator-prog deployment experiment | Runtime, oracle, CU, and risk checks pass; no real collateral |
| P3 | Privacy compatibility lab | Syscall and proof-program conformance matrix | Positive and negative proof vectors match upstream behavior |

The included code addresses P1 and testing. It is runnable research code, not a production patch. The two standalone models passed seven tests locally. The reconstructed repository harness has earlier-review execution evidence at `7e41b544`, but has not been rebuilt against `7703b91a` in this package. See each README for limitations.

## 1. Consensus, independently verified finality, and formal models

**Problem.** A fast slot cadence is not independently verified finality. The earlier source review of `7e41b544` identified a follower trust-anchor gap: gateway-provided identities and certificate summary fields were being trusted. Recheck this finding against the current implementation before proposing a patch.

**Methodology.** Anza's Alpenglow design combines Votor's concurrent one/two-round voting paths with Rotor's erasure-coded dissemination. The May 2025 announcement reports approximately 150 ms median finality in simulation, explicitly excluding computation overhead. It describes 80% participation for the one-round path, 60% responsiveness for two rounds, and a 20% adversarial plus 20% nonresponsive resilience model. These are protocol-specific thresholds, not constants to copy into Acrux. The source page now links a July 2026 v1.2 white paper; this pack has not evaluated the differences between paper versions.

**Acrux experiment.** Write a small executable consensus state-machine model with explicit assumptions: validator membership, stake weights, epoch transition, authenticated messages, partitions, and timing. Exhaustively explore small validator sets and adversarial schedules. Assert no two conflicting blocks finalize in one slot. Separately test liveness under the stated fault bounds. Formal model checking and fuzz tests cover different failure classes; neither alone proves the deployed implementation correct.

For follower verification, establish an independent genesis/checkpoint anchor, derive or verify membership changes, and verify signed votes rather than trusting certificate summary JSON. Add negative cases for duplicate signers, wrong epoch, wrong chain, incorrect weights, and forged proposer membership. Measure network, dissemination, execution, and voting latency separately, including p95/p99 and recovery after partition.

Sources: https://www.anza.xyz/blog/alpenglow-a-new-consensus-for-solana and https://github.com/solana-foundation/solana-improvement-documents/blob/main/proposals/0326-alpenglow.md

## 2. Stateful property tests, fuzzing, and bounded resources

**Problem.** The earlier review found permanently retained threshold transaction IDs, subscription-driven repeated reads, and unbounded verification queues. These are resource-policy questions as well as parser questions. Confirm the current code paths before integration.

**Methodology.** Combine generated operation sequences and a reference model with coverage-guided `cargo-fuzz` campaigns. Stellar documents `cargo-fuzz`, `arbitrary`, and conversion to `proptest` via interop. Keep minimized failing inputs in a regression corpus. Use process-level timeouts and memory limits: `catch_unwind` does not catch allocator aborts or prove freedom from hangs.

**Included harness.** `code/harness` reconstructs seeded mutation tests for fragment parsing, threshold cache operations, payload/envelope decoding, and a tombstone serialization measurement. These are seeded mutation tests, not shrinking proptest strategies or a coverage-guided campaign. The operation stream has fixed mutation seeds, but fixture keys and hashes are generated randomly, so exact input bytes vary. A green run only covers the tested inputs.

**Included models.**
- `poc-tombstone-horizon`: illustrates retention by an agreed horizon. It does NOT prove the original transaction target slot bounds blockhash validity. Integration requires an authenticated expiry bound, a deterministic stale-input rule shared by all replicas, future-slot restrictions, and a state migration plan. Never remove replay protection solely because this model passes.
- `poc-subscription-delta`: illustrates dirty-account intersection and coalescing. Its simplified buckets are per connection, NOT the existing shared per-IP limiter. Production code must share client budgets across connections, bound batch cost and dirty sets, and preserve pending changes rather than silently dropping updates under rate limits.

Acceptance criteria: validator/follower state equality at every checkpoint, replay rejection across restarts, no notification loss after temporary throttling, bounded memory under sustained load, and published p99 slot-delay measurements. Changing consensus-state retention needs protocol review, not a gateway-only patch.

Source: https://developers.stellar.org/docs/build/smart-contracts/example-contracts/fuzzing

## 3. Parallel execution under contention

**Problem.** Declared account access permits parallel scheduling, but a hot writable account still serializes conflicting transactions. An observed low TPS value is not a capacity benchmark and should not be used to infer executor limits.

**Methodology.** Compare a serial baseline with static conflict scheduling and an optimistic executor. The NEMO preprint (16 October 2025) combines OCC with an object model, partial read/write hints, dependency handling, and unblock-priority scheduling. Its authors report up to 42% higher throughput than Block-STM with 16 workers in simulated execution experiments. That number is not an Acrux performance prediction; an SVM account model does not inherit an owned-object fast path automatically.

**Acrux experiment.** Sweep worker count, hot-key fraction, account fanout, failed transactions, and program mix. Treat declared writable accounts as a validation boundary, not an unchecked scheduler hint. Compare state hashes, fees, error results, and order-sensitive effects with serial execution. Measure re-execution count, memory, fairness, p99 slot completion, and useful committed TPS. A single-slab perp market is an intentionally contended workload for this test.

Sources: https://arxiv.org/abs/2510.15122 and https://solana.com/en/news/sealevel---parallel-processing-thousands-of-smart-contracts

## 4. Perpetuals DEX compatibility and risk lab

**Problem.** A perpetuals program is a useful SVM compatibility workload, but compatibility and shorter slots do not establish economic safety or faster oracle data.

**Methodology.** `percolator-prog` wraps the percolator v16 risk engine. Its current README describes a multi-asset slab, per-position bounded work, matcher CPI trust boundaries, and CU regression tests, including 14-leg trades. It explicitly warns that the program is unaudited, educational, and unsuitable for real funds. The Apache-2.0 license permits reuse subject to its terms. The upstream repository is moving quickly, so pin both program and engine revisions.

**Acrux experiment.** First run upstream tests unchanged. Then compare loader/ABI behavior, account-size limits, syscalls, CPI signer binding, compute metering, and transaction results on local Acrux versus the upstream test runtime. Inventory every clock dependency: do not multiply all slot parameters by eight blindly. Oracle freshness, funding, expiry, liquidation, and wall-clock semantics require separate analysis. Add economic scenarios for stale oracles, price jumps, liquidator outages, rounding, insolvency, and concurrent orders.

Hyperliquid's documentation is useful as a design comparison for margin checks at order entry and matching, and order-flow sequencing. Its throughput figures are operator claims measured in orders, not comparable directly to chain TPS. A faster chain does not remove oracle, fairness, or shared-account bottlenecks.

Sources: https://github.com/aeyakovenko/percolator-prog and https://hyperliquid.gitbook.io/hyperliquid-docs/llms-full.txt

## 5. Bridge safety after the 0.4.12 changes

**Already shipped in source.** Commit `7703b91a` adds keeper relaying, proof persistence, withdrawal release, and epoch light-client handoff, alongside initialization authority checks, validator-set transition restrictions, dual-RPC deposit agreement, mint allowlisting, identity-seed checks, and invalid-credit handling. Source availability is not evidence of mainnet deployment. Do not reopen these implementation tasks without first checking their current behavior.

**Methodology.** Separate safety from liveness and enumerate trust assumptions in each bridge direction. The 2025 bridge study emphasizes recurring access-control, validator-trust, and verification failures and discusses layered checks and circuit breakers. Light-client verification can reduce reliance on an external signer set, but is not immune to implementation, upgrade, membership, or source-consensus failures. Do not interpret an absence of reported incidents as a safety proof.

**Acrux experiment.** Test duplicated/competing keepers, keeper outage, crash immediately before/after transaction submission, replay after restart, validator-set handoff, stale epochs, skipped epochs, and divergent RPC observations. Ensure a compromised relayer cannot substitute a proof or authorize a withdrawal. Two RPCs may share upstream infrastructure; agreement is a mitigation, not independent cryptographic proof. Measure time to detect and halt, and evaluate value-at-risk caps with explicit availability tradeoffs. These are research checks, not claims that all such controls are currently missing.

Solayer's documentation offers a useful replay-protection, cap, and recovery checklist, but presents both persistent-log and database-free descriptions. Resolve the actual deployed architecture before adopting its implementation details.

Sources: https://github.com/AcruxComputer/acrux/commit/7703b91ac3a2f872667d1f958f63f4a81b4ad151, https://arxiv.org/abs/2507.06156, and https://docs.solayer.org/documentation/solayer-bridge/security-measures

## 6. Privacy and ZK: prove compatibility before promising a port

**Methodology.** Helius attributes Poseidon and alt_bn128 syscall work and ZK Compression to Light, and its acquisition page says Privacy Rings is live. Those are vendor statements. This pack does not independently establish deployment addresses, current audits, production usage, or availability of a portable open-source Rings stack. ZK Compression reduces state cost; it is not confidentiality by itself. Confidential balances are another mechanism and should not be treated as interchangeable with compression or Rings.

**Acrux experiment.** Build a matrix of required curve/hash/proof syscalls, feature gates, compute pricing, program IDs, token extensions, client SDKs, prover services, and indexing infrastructure. Run upstream valid and invalid proof vectors locally. Check malformed encodings, subgroup/point validation, out-of-range values, and deterministic metering. Benchmark proof generation separately from on-chain verification. Only after that attempt a private devnet transfer flow with nonvaluable assets. Selective disclosure and key recovery require their own product and security decisions.

Sources: https://www.helius.dev/blog/light-protocol-acquisition, https://www.helius.dev/blog/confidential-balances, and https://www.helius.dev/blog/solana-builders-zk-compression

## Scope and reuse

The roadmap distinguishes source-backed design descriptions, vendor performance claims, preprint measurements, and proposed experiments. It does not certify any chain, bridge, proof system, or derivatives program. Research models should first be adapted and tested on local networks. Do not use real funds with this package. Code is supplied for review and adaptation; no upstream patch has been applied by this repository.
