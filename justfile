# Common workflows. Run `just` to see the list.

default:
    @just --list

# Run a command with a process-tree cap (default 2 GiB), no swap, and a three-minute timeout.
# Examples: just safe cargo test --workspace; just safe just eval repro.cue
# Historical pre-optimization original Odoo runs need CUE_SAFE_MEMORY=3G; current runs fit 2 GiB.
[positional-arguments]
safe *COMMAND:
    systemd-run --user --scope -p "MemoryMax=${CUE_SAFE_MEMORY:-2G}" -p MemorySwapMax=0 -p TasksMax=256 timeout --kill-after=5s 180s env CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=1 prlimit --core=0 -- "$@"

# Workspace tests.
test *ARGS:
    cargo test --workspace --no-fail-fast {{ARGS}}

# One crate's tests, e.g. `just test-crate cue-eval`.
test-crate CRATE *ARGS:
    cargo test -p {{CRATE}} --no-fail-fast {{ARGS}}

# Clippy over everything, warnings denied.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Format the crates this repo owns.
fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

# Legacy heuristic corpus checks. These are not conformance acceptance.
txtar DIR="tests/testdata":
    cargo run --release -p cue-cli -- test-txtar {{DIR}} --strict-errors

# Names of legacy failures (preserves the runner's nonzero exit status).
txtar-failures DIR="tests/testdata":
    #!/usr/bin/env bash
    set -o pipefail
    cargo run --release -q -p cue-cli -- test-txtar {{DIR}} --strict-errors | awk '/^Running/{name=$2} /FAILED/{print name}'

# Build the pinned Go oracle. Run with `just safe just oracle-build`.
oracle-build:
    mkdir -p tmp/go-mod-cache tmp/go-build-cache
    env GOMODCACHE="{{justfile_directory()}}/tmp/go-mod-cache" GOCACHE="{{justfile_directory()}}/tmp/go-build-cache" go -C tools/conformance-oracle build -p 1 -trimpath -o ../../tmp/conformance-oracle .

# Reference adapter tests; Go dependencies are pinned in go.mod/go.sum.
oracle-test:
    env GOMODCACHE="{{justfile_directory()}}/tmp/go-mod-cache" GOCACHE="{{justfile_directory()}}/tmp/go-build-cache" go -C tools/conformance-oracle test -p 1 ./...

# Strict acceptance: failures AND unsupported verification return nonzero.
[positional-arguments]
conformance FIXTURE_PATH="tests/testdata" *ARGS:
    cargo run -p cue-cli -- conformance "$@" --manifest tests/conformance/manifest.json

# Compare the checked-in migration baseline; this is not conformance acceptance.
conformance-baseline:
    cargo run -p cue-cli -- conformance tests/testdata --manifest tests/conformance/manifest.json --baseline tests/conformance/baseline.json

# Run a semantic family selected by filename (e.g. definitions or builtins).
conformance-family FAMILY:
    cargo run -p cue-cli -- conformance tests/testdata --manifest tests/conformance/manifest.json --filter '{{FAMILY}}'

# Real-reference integration controls, after oracle-build.
conformance-test:
    env CUE_CONFORMANCE_ORACLE="{{justfile_directory()}}/tmp/conformance-oracle" cargo test -p cue-cli --test conformance -- --include-ignored

# Ingest upstream test suites from a local CUE checkout.
sync-upstream CHECKOUT:
    cargo run -p cue-cli -- sync-upstream --src {{CHECKOUT}}

# Evaluate a CUE file with this engine.
eval FILE *ARGS:
    cargo run -q -p cue-cli -- eval {{FILE}} {{ARGS}}

# Sequential release RSS/time measurements. Run with `just safe just benchmark-odoo PATH`.
benchmark-odoo REPRO RUNS="3":
    cargo build --release -p cue-cli
    python3 tools/benchmark-odoo.py '{{REPRO}}' --runs '{{RUNS}}'

# What CI would check.
ci: fmt-check lint test oracle-build oracle-test conformance-test conformance-baseline
