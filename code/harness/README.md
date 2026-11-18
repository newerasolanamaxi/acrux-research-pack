# Repository mutation-test harness

Reconstructed from an earlier review of Acrux revision
`7e41b5446dc333e42655d3ed45669e82b99cdaac`. Earlier review execution reported
all four test functions passing. This reconstructed package has NOT been
rebuilt against the newer `7703b91a` revision. API adjustments may be needed.

## Layout and run

Keep the complete research pack next to the Acrux clone:

```
work/
  acrux/
  acrux-research-pack/
    code/harness/
```

Use the reviewed revision for reproducing the earlier tests:

```
git clone https://github.com/AcruxComputer/acrux.git
cd acrux
git checkout 7e41b5446dc333e42655d3ed45669e82b99cdaac
cd ../acrux-research-pack/code/harness
cargo test -- --nocapture
```

The dependency build may be large. The tests do not start a node or contact a
live RPC. Cargo will download dependencies on the first build. No lockfile is
included, so resolved transitive versions can change.

## Coverage and limitations

- fuzz_threshold.rs: 20,000 parser mutations and 5,000 cache operations.
- fuzz_payload.rs: random bytes, structured payload mutations, and hostile
  length prefixes. The Envelope path is tested on random bytes; the structured
  mutations apply to BlockPayload only.
- tombstone_growth.rs: serialization-size measurement of retained transaction
  IDs, including cloned serialized cache sizes. Serialized bytes are NOT an
  allocator/RSS measurement of checkpoint heap use.

These are seeded mutation loops, not proptest strategies with shrinking and
not coverage-guided fuzzing. Fixture keys/hashes remain randomized. Some
mutation paths can leave valid inputs, so acceptance is not itself a bug.
Catch-unwind checks do not catch OOM aborts or impose execution timeouts.
Run under OS resource/time limits for longer campaigns. A green run establishes
only the properties exercised by those inputs, not parser or consensus safety.

The tombstone-growth test asserts behavior at the reviewed revision. Once
retention behavior changes, adapt this historical regression measurement.
