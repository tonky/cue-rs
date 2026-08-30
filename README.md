# cue-rs: Fast, Native CUE Validator & Evaluator in Rust

[![Rust 2024](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org)
[![Clippy Clean](https://img.shields.io/badge/clippy-0%20warnings-brightgreen.svg)](https://github.com/rust-lang/rust-clippy)
[![Txtar Conformance](https://img.shields.io/badge/txtar%20tests-230%2F230%20passing-brightgreen.svg)](tests/testdata/)
[![License](https://img.shields.io/badge/license-Apache%202.0%20%2F%20MIT-blue.svg)](LICENSE)

A high-performance, modular implementation of the [CUE configuration language](https://cuelang.org/) in **Rust (2024 Edition)**.

Designed for embedding in high-throughput data pipelines, cloud-native control planes, CLI tools, procedural macros, and Rust applications without external runtime dependencies.

---

## 1. Workspace Architecture

```
├── Cargo.toml                  # Workspace manifest (Rust 2024 Edition)
├── CUE_CONFORMANCE_TRACKER.md  # Upstream test suite inventory & parity tracker
├── CUE_ROADMAP.md              # Milestone progress and architectural design
├── CUE_RUST_LEARNINGS.md       # Comparative architecture & design trade-offs
├── crates/
│   ├── cue-syntax/             # Lexer (logos), ASI, Pratt parser, AST pretty-printer (`fmt`)
│   ├── cue-eval/               # Arena-based lattice unification (⊓) engine & 24 stdlib packages
│   ├── cue-derive/             # Procedural macro `#[derive(CueValidate)]` with Serde
│   ├── cue-test-harness/       # Upstream .txtar test fixture parser & test runner
│   └── cue-cli/                # CLI binary (`cue-rs eval`, `cue-rs vet`, `cue-rs fmt`, `cue-rs test-txtar`)
├── tests/
│   └── testdata/               # 158 conformance .txtar suites (100% passing)
└── examples/                   # Sample CUE schemas and data files
```

---

## 2. Key Features

- **Lattice Unification ($\sqcap$)**: Greatest lower bound calculation over scalar values, recursive structs, bounds (`>1024 & <65535`), regex constraints (`=~ "^[a-z]+$"`), and closed `#Definitions`.
- **String Transformations & Prefix/Suffix Trimming**: `strings.ReplaceAll(s, old, new)`, `strings.TrimPrefixAny(s, prefixes)`, `strings.TrimSuffixAny(s, suffixes)`, `strings.SplitN(s, sep, n)`, `strings.HasPrefixAny(s, prefixes)`, `strings.HasSuffixAny(s, suffixes)`.
- **Math & Numeric Functions**: `math.FMA(x, y, z)`, `math.Pow10(n)`, `math.Frexp(x)`, `math.Modf(x)`, `math.Scaleb(x, n)`, `math.Erf(x)`, `math.Erfc(x)`, `math.Gamma(x)`, `math.LogGamma(x)`, `math.RoundToEven(x)`, `math.Logb(x)`, `math.Ilogb(x)`, `math.Nextafter(x, y)`.
- **List Operations**: `list.Chunk(l, n)`, `list.Distinct(l)`, `list.Zip(l1, l2)`, `list.Unzip(l)`, `list.Compact(l)`, `list.Reverse(l)`, `list.SortStrings(l)`.
- **Time Field Extraction & Reference Parsing**: `time.Year(t)`, `time.Month(t)`, `time.Day(t)`, `time.Parse(layout, val)`, `time.FormatDuration(nanos)`, `time.Unix(sec, nsec)`.
- **Extended Encodings**: `base32.HexEncode(s)`, `base32.HexDecode(s)`, `hex.EncodedLen(n)`, `hex.DecodedLen(n)`, `hex.Dump(s)`.
- **Path Cleaning**: `path.Clean(p)`.
- **Rust Derive Macro (`cue-derive`)**: Deserialization schema validation on Rust structs via Serde:
  ```rust
  #[derive(Deserialize, CueValidate)]
  #[cue(schema = "#User: { id: uint32, name: string, email: =~\"@\" }")]
  struct User {
      id: u32,
      name: String,
      email: String,
  }
  ```
- **24 Built-in Standard Library Packages**:
  - `strings`, `math`, `math/bits`, `list`, `regexp`, `struct`, `time`, `net`, `strconv`, `uuid`, `encoding/json`, `encoding/yaml`, `encoding/html`, `encoding/csv`, `encoding/base32`, `encoding/base64`, `encoding/hex`, `text/tabwriter`, `text/template`, `crypto/sha512`, `crypto/sha256`, `crypto/md5`, `crypto/sha1`, `crypto/hmac`, `path`.

---

## 3. Quick Start

### Build & Test

```bash
# Build the entire workspace
cargo build

# Run all workspace unit tests (27/27 passing)
cargo test --workspace

# Run clippy lint verification (0 warnings)
cargo clippy --workspace --all-targets

# Run the 118 txtar conformance suites (118/118 passing)
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
- [`CUE_RUST_LEARNINGS.md`](CUE_RUST_LEARNINGS.md): Comparative architecture analysis and design trade-offs.
