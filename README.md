# cue-rs: Fast, Native CUE Validator & Evaluator in Rust

[![Rust 2024](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org)
[![Clippy Clean](https://img.shields.io/badge/clippy-0%20warnings-brightgreen.svg)](https://github.com/rust-lang/rust-clippy)
[![Txtar Conformance](https://img.shields.io/badge/txtar%20tests-112%2F112%20passing-brightgreen.svg)](tests/testdata/)
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
│   └── testdata/               # 112 conformance .txtar suites (100% passing)
└── examples/                   # Sample CUE schemas and data files
```

---

## 2. Key Features

- **Lattice Unification ($\sqcap$)**: Greatest lower bound calculation over scalar values, recursive structs, bounds (`>1024 & <65535`), regex constraints (`=~ "^[a-z]+$"`), and closed `#Definitions`.
- **Lexical Path Cleaning & Multi-Split**: `path.Clean(p)`, `strings.SplitN(s, sep, n)`, `strings.HasPrefixAny(s, prefixes)`, `strings.HasSuffixAny(s, suffixes)`.
- **Advanced Math Functions**: `math.RoundToEven(x)`, `math.Logb(x)`, `math.Ilogb(x)`, `math.Nextafter(x, y)`, `math.Mod(x, y)`, `math.Ldexp(f, exp)`, `math.IsNaN(x)`, `math.IsInf(x)`.
- **Time Format & Reference Parsing**: `time.Parse(layout, val)`, `time.FormatDuration(nanos)`, `time.Unix(sec, nsec)`.
- **Regex & Network Utilities**: `regexp.QuoteMeta(s)`, `regexp.FindSubmatch(pat, s)`, `net.FQDN(s)`, `net.ParseIP(s)`, `net.SplitHostPort(s)`, `net.JoinHostPort(h, p)`.
- **Hex & Base Encoding Helpers**: `hex.EncodedLen(n)`, `hex.DecodedLen(n)`, `hex.Dump(s)`, `hex.Encode`, `hex.Decode`.
- **Hyperbolic Math & Bitwise Logic**: `math.Sinh`, `math.Cosh`, `math.Tanh`, `math.Asinh`, `math.Acosh`, `math.Atanh`, `bits.Len`, `bits.LeadingZeros`, `bits.TrailingZeros`, `bits.Reverse`.
- **String Cutset Trimming & Sorting**: `strings.TrimSpace(s)`, `strings.TrimLeft(s, cutset)`, `strings.TrimRight(s, cutset)`, `list.SortStrings(l)`, `list.Reverse(l)`, `list.Compact(l)`.
- **JSON & YAML Validation Helpers**: `json.Valid(s)`, `json.Validate(s, schema)`, `yaml.Valid(s)`, `yaml.Validate(s, schema)`, `json.Indent`, `json.Compact`.
- **Extended Math Powers & Roots**: `math.Cbrt(x)`, `math.Exp(x)`, `math.Exp2(x)`, `math.Expm1(x)`, `math.Log1p(x)`, `math.Sign(x)`, `math.Dim(x, y)`, `math.Copysign(x, y)`.
- **Deep List Operations**: `list.Flatten(l)` recursive flattening, `list.Slice(l, low, high)`, `list.Concat([l1, l2])`, `list.Repeat(elem, count)`.
- **Logical Boolean Operators**: Full `&&` (logical AND) and `||` (logical OR) support across expressions, conditionals, and comprehensions.
- **Module-Aware Package Imports**: Seamless resolution and evaluation of module packages (`import "myorg.com/app/schema"`) and vendored packages (`cue.mod/pkg/...`).
- **Inter-Arena Deep Value Cloning (`clone_value_into`)**: Recursive value allocation across isolated package evaluation arenas.
- **Parenthesized Selector & Index Chaining**: `({ cluster: { id: "p1" } }).cluster.id`, `(["alpha", "beta"])[1]`, `(inlineMap["prod"]).ports[1]`.
- **Comprehensions with `let` Bindings & Dynamic Labels**: `for k, v in map let uk = strings.ToUpper(k) if strings.HasPrefix(uk, "P_") { (strings.ToLower(uk)): v }`.
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

# Run the 112 txtar conformance suites (112/112 passing)
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
