# cue-rs: Fast, Native CUE Validator & Evaluator in Rust

[![Rust 2024](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org)
[![Clippy Clean](https://img.shields.io/badge/clippy-0%20warnings-brightgreen.svg)](https://github.com/rust-lang/rust-clippy)
[![Txtar Conformance](https://img.shields.io/badge/txtar%20tests-84%2F84%20passing-brightgreen.svg)](tests/testdata/)
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
│   └── testdata/               # 84 conformance .txtar suites (100% passing)
└── examples/                   # Sample CUE schemas and data files
```

---

## 2. Key Features

- **Lattice Unification ($\sqcap$)**: Greatest lower bound calculation over scalar values, recursive structs, bounds (`>1024 & <65535`), regex constraints (`=~ "^[a-z]+$"`), and closed `#Definitions`.
- **Logical Boolean Operators**: Full `&&` (logical AND) and `||` (logical OR) support across expressions, conditionals, and comprehensions.
- **Path Matching & Inspection**: `path.Match(pattern, path)`, `path.Split(path)`, `path.IsAbs(path)`, `path.Base`, `path.Dir`, `path.Ext`, `path.Join`.
- **Extended Strings & Math Functions**: `strings.Count(s, sub)`, `strings.Title(s)`, `math.Sign(x)`, `math.Dim(x, y)`, `math.Copysign(x, y)`, `list.Slice(l, low, high)`.
- **Module-Aware Package Imports**: Seamless resolution and evaluation of module packages (`import "myorg.com/app/schema"`) and vendored packages (`cue.mod/pkg/...`).
- **Inter-Arena Deep Value Cloning (`clone_value_into`)**: Recursive value allocation across isolated package evaluation arenas.
- **Parenthesized Selector & Index Chaining**: `({ cluster: { id: "p1" } }).cluster.id`, `(["alpha", "beta"])[1]`, `(inlineMap["prod"]).ports[1]`.
- **Comprehensions with `let` Bindings & Dynamic Labels**: `for k, v in map let uk = strings.ToUpper(k) if strings.HasPrefix(uk, "P_") { (strings.ToLower(uk)): v }`.
- **String Functions & Escape Sequences**: `strings.Fields(s)`, `strings.Split(s, sep)`, `strings.Index(s, sub)`, `strings.LastIndex(s, sub)`, `strings.Compare(a, b)`, and full escape sequence support (`\"`, `\\`, `\n`, `\t`).
- **JSON Formatting & Minification**: `encoding/json.Indent(s, prefix, indent)`, `encoding/json.Compact(s)`.
- **Extended Math Package**: `math.Hypot(p, q)`, `math.Log10(x)`, `math.Log2(x)`, `math.Trunc(f)`, `math.Round(f)`, `math.Floor(f)`, `math.Ceil(f)`.
- **List Aggregation & Manipulation**: `list.Concat([l1, l2])`, `list.Repeat(elem, count)`, `list.Sum`, `list.Product`, `list.Avg`, `list.Min`, `list.Max`.
- **Time Package Unix Formatter & Constants**: `time.Unix(sec, nsec)`, `time.Hour`, `time.Minute`, `time.Second`, `time.Millisecond`, `time.Microsecond`, `time.Nanosecond`.
- **Strconv String Escaping & Arbitrary Base Formatting**: `strconv.FormatInt(i, base)`, `strconv.Quote(s)`, `strconv.Unquote(s)`.
- **Struct Embedding with Disjunction Selection**: Embedded disjunction schemas (`#Prod | #Dev`) resolving via field unification.
- **Comprehensions with Standard Library Filtering**: Iteration with stdlib functions in conditions (`strings.HasPrefix`, `path.Match`) and mapping expressions (`strings.ToUpper`, `strings.TrimPrefix`, `strings.Replace`).
- **Optional Field Validation & Export Filtering**: Validates `field?: type` when present and omits unpopulated optional fields from JSON/YAML export.
- **Hidden Fields & Definitions Isolation**: Evaluates `_internal` and `#Schema` identifiers in scope while filtering them from output.
- **Discriminated Union Disjunctions**: Pattern and tag-based union branch resolution (`#Circle | #Rectangle`).
- **Mixed Numeric Bounds**: Multi-constraint numeric bounds with seamless int/float comparisons (`number & >0` matching `12.5` and `42`).
- **Order-Independent Reference Relaxation**: Multi-pass fixpoint evaluation resolving forward field references and mutual dependencies (`a: b + 1, b: c * 2, c: 10`).
- **Binary, Hex, Octal & SI Number Literals**: `0b1100`, `0x2A`, `0o755`, `4Ki`, `10M`, `2G`.
- **Raw & Multi-Line Strings**: Single-line raw strings `#"..."#`, multi-line `#"""..."""#`, and raw bytes `#'...'#`.
- **Nested Dynamic Indexing & Selector Chains**: Dynamic struct & array lookup (`database.environments[targetEnv].pool[tierIndex]`), deep selector chaining (`cluster.ingress.tls.secret`), and list slicing (`items[1:]`, `items[:3]`, `items[2:5]`).
- **Fixed-Width Numeric Hierarchy**: Full range validation for `uint`, `uint8`, `uint16`, `uint32`, `uint64`, `int8`, `int16`, `int32`, `int64`, `float32`, `float64`.
- **List & String Arithmetic**: List concatenation (`[1, 2] + [3, 4]`), list repetition (`[0] * 4`), and string repetition (`"=" * 10`).
- **Open List Ellipsis**: Seamless unification of open lists (`#IntList: [...int]`) with concrete instances.
- **Disjunction Meet Algebra**: Cross-product branch unification with transactional backtracking (`checkpoint()` / `rollback()`).
- **Cartesian Product List Comprehensions**: Index unpacking (`[ for i, x in s1 for j, y in s2 { ... } ]`) and struct-body list comprehensions (`[ for k, v in map { name: k, port: v.port } ]`).
- **Multi-Pattern Constraints**: Simultaneous regex pattern constraints on structs (`[=~"^STR_"]: string`, `[=~"^NUM_"]: int & >0`, `[=~"^FLAG_"]: bool`).
- **Module Discovery (`cue.mod/module.cue`)**: Upward directory traversal extracting module name and language version into `ModuleInfo`.
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

# Run the 84 txtar conformance suites (84/84 passing)
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
