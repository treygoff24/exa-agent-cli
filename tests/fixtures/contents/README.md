# Contents compatibility fixture provenance

`legacyEnvelope` values were captured from the release binary built at local
`main` commit `36f10803d837b1804b8e23761e9cefddc47f354b` (the pre-Wave 5
contract), not from the implementation under test.

The capture used these commands on 2026-07-16:

```sh
git worktree add --detach /tmp/exa-agent-main-wave5 36f10803d837b1804b8e23761e9cefddc47f354b
cargo build --release --manifest-path /tmp/exa-agent-main-wave5/Cargo.toml

# For each fixture with a legacyEnvelope, a one-request loopback HTTP server
# returned `jq -c .upstream "$fixture"` from POST /contents. Then:
/tmp/exa-agent-main-wave5/target/release/exa-agent \
  "${argv[@]}" \
  --api-key test-key-abcdef12 \
  --base-url "$loopback_url" \
  --compact \
| jq '.request.requestId = "req_legacy" | .diagnostics.durationMs = 0'
```

`argv` is the fixture's `argv` array. The loopback server was equivalent to
`tests/cli.rs::local_json_server`: it returned the fixture's `upstream` object
as an HTTP 200 JSON body. Only the generated request ID and elapsed duration
were normalized.
