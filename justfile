# Common workflows. Run `just` to see the list.

default:
    @just --list

# Run a command with a process-tree cap (default 2 GiB), no swap, and a three-minute timeout.
# Examples: just safe cargo test --workspace; just safe just eval repro.cue
# For the original Odoo snapshot: CUE_SAFE_MEMORY=3G just safe just eval ../cue-rs-odoo-repro/original
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

# The upstream-derived conformance corpus. Takes a file or a directory.
txtar DIR="tests/testdata":
    cargo run --release -p cue-cli -- test-txtar {{DIR}}

# Names of the corpus cases that currently fail.
txtar-failures DIR="tests/testdata":
    @cargo run --release -q -p cue-cli -- test-txtar {{DIR}} | awk '/^Running/{name=$2} /FAILED/{print name}'

# Ingest upstream test suites from a local CUE checkout.
sync-upstream CHECKOUT:
    cargo run -p cue-cli -- sync-upstream {{CHECKOUT}}

# Evaluate a CUE file with this engine.
eval FILE *ARGS:
    cargo run -q -p cue-cli -- eval {{FILE}} {{ARGS}}

# What CI would check.
ci: fmt-check lint test txtar
