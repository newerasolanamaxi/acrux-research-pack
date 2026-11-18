# Acrux research pack

Research directions and runnable local experiments for developers evaluating
Acrux. Read [RESEARCH_DIRECTIONS.md](RESEARCH_DIRECTIONS.md) first.

## Contents

- `code/harness`: reconstructed tests driving repository crates at the earlier
  reviewed revision `7e41b544`. Not rebuilt against current `7703b91a`.
- `code/poc-tombstone-horizon`: standalone retention-policy model, four tests.
- `code/poc-subscription-delta`: standalone notification batching model, three tests.

The two standalone crates passed `cargo test` locally. They are research
models, NOT production fixes or patches ready to merge. Their READMEs explain
security assumptions and missing integration behavior. The roadmap recognizes
the bridge changes in node 0.4.12 / contract 0.1.18 without claiming deployment.

## Quick start

Each standalone crate requires Rust/Cargo and no third-party dependencies:

```
cd code/poc-subscription-delta
cargo test
```

For the repository harness, use its separate layout and pinned-revision
instructions. No node or live traffic is started by these tests.

Do not use real funds. This package is not an audit or a safety certificate.
