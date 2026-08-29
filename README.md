# cue-rs: Fast, Native CUE Validator & Evaluator in Rust

[![Rust 2024](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org)
[![Clippy Clean](https://img.shields.io/badge/clippy-0%20warnings-brightgreen.svg)](https://github.com/rust-lang/rust-clippy)
[![Txtar Conformance](https://img.shields.io/badge/txtar%20tests-47%2F47%20passing-brightgreen.svg)](tests/testdata/)
[![License](https://img.shields.io/badge/license-Apache%202.0%20%2F%20MIT-blue.svg)](LICENSE)

A high-performance, modular implementation of the [CUE configuration language](https://cuelang.org/) in **Rust (2024 Edition)**.

Designed for embedding in high-throughput data pipelines, cloud-native control planes, CLI tools, procedural macros, and Rust applications without external runtime dependencies.

---

## 1. Workspace Architecture

```
├── Cargo.toml                  # Workspace manifest (Rust 2024 Edition)
├── CUE_CONFORMANCE_TRACKER.md  # Upstream test suite inventory & parity tracker
├── CUE_ROADMAP.md              # Milestone progress and architectural design
├── crates/
│   ├── cue-syntax/             # Lexer (logos), ASI, Pratt parser, AST pretty-printer (`fmt`)
│   ├── cue-eval/               # Arena-based lattice unification (⊓) engine & 17 stdlib packages
│   ├── cue-derive/             # Procedural macro `#[derive(CueValidate)]` with Serde
│   ├── cue-test-harness/       # Upstream .txtar test fixture parser & test runner
│   └── cue-cli/                # CLI binary (`cue-rs eval`, `cue-rs vet`, `cue-rs fmt`, `cue-rs test-txtar`)
├── tests/
│   └── testdata/               # 47 conformance .txtar suites (100% passing)
└── examples/                   # Sample CUE schemas and data files
```

---

## 2. Key Features

- **Lattice Unification ($\sqcap$)**: Greatest lower bound calculation over scalar values, recursive structs, bounds (`>1024 & <65535`), regex constraints (`=~ "^[a-z]+$"`), and closed `#Definitions`.
- **Order-Independent Reference Relaxation**: Multi-pass fixpoint evaluation resolving forward field references and mutual dependencies (`a: b + 1, b: c * 2, c: 10`).
- **Raw & Multi-Line Strings**: Single-line raw strings `#"..."#`, multi-line `#"""..."""#`, and raw bytes `#'...'#`.
- **Dynamic Indexing & Selector Chains**: Dynamic struct lookup (`ports[currentEnv]`) and deep selector chaining (`cluster.ingress.tls.secret`).
- **Fixed-Width Numeric Hierarchy**: Full range validation for `uint`, `uint8`, `uint16`, `uint32`, `uint64`, `int8`, `int16`, `int32`, `int64`, `float32`, `float64`.
- **List & String Arithmetic**: List concatenation (`[1, 2] + [3, 4]`), list repetition (`[0] * 4`), and string repetition (`"=" * 10`).
- **Open List Ellipsis**: Seamless unification of open lists (`#IntList: [...int]`) with concrete instances.
- **Disjunction Meet Algebra**: Cross-product branch unification with transactional backtracking (`checkpoint()` / `rollback()`).
- **Comprehensions & Dynamic Keys**: Chained multi-clause comprehensions (`for`, `if`, `let`), Cartesian product list comprehensions (`[ for x in s1 for y in s2 { ... } ]`), struct-body list comprehensions (`[ for k, v in map { name: k, port: v.port } ]`), and dynamic interpolated labels (`(key): val`, `"\(k)_env": val`, `("item_\(i)"): val`).
- **Rust Derive Macro (`cue-derive`)**: Automatic deserialization-time schema validation on Rust structs via Serde:
  ```rust
  #[derive(Deserialize, CueValidate)]
  #[cue(schema = "#User: { id: uint32, name: string, email: =~\"@\" }")]
  struct User {
      id: u32,
      name: String,
      email: String,
  }
  ```
- **17 Built-in Standard Library Packages**:
  - `strings`, `math`, `list`, `regexp`, `struct`, `time`, `net`, `strconv`, `uuid`, `encoding/json`, `encoding/yaml`, `encoding/csv`, `encoding/base64`, `encoding/hex`, `crypto/sha256`, `crypto/md5`, `crypto/sha1`, `path`.

---

## 3. Quick Start

### Build & Test

```bash
# Build the entire workspace
cargo build

# Run all workspace unit tests (25/25 passing)
cargo test --workspace

# Run clippy lint verification (0 warnings)
cargo clippy --workspace --all-targets

# Run the 47 txtar conformance suites (47/47 passing)
cargo run -p cue-cli -- test-txtar tests/testdata
```

### CLI Commands (`cue-rs`)

```bash
# 1. Evaluate a CUE file (JSON or YAML format)
cargo run -p cue-cli -- eval examples/data.cue --format json
cargo run -p cue-cli -- eval examples/data.cue --format yaml

# 2. Validate (vet) a data file against a schema definition
cargo run -p cue-cli -- vet examples/schema.cue examples/data.cue

# 3. Format CUE source code
cargo run -p cue-cli -- fmt examples/data.cue

# 4. Run upstream .txtar test suites
cargo run -p cue-cli -- test-txtar tests/testdata
```

---

## 4. Documentation & Roadmap

- [`CUE_CONFORMANCE_TRACKER.md`](CUE_CONFORMANCE_TRACKER.md): Upstream CUE test inventory, feature comparison, and conformance tracking.
- [`CUE_ROADMAP.md`](CUE_ROADMAP.md): Detailed phase breakdown, memory model, and milestone progress.
