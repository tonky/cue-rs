# cue-rs: Fast, Native CUE Validator & Evaluator in Rust

[![Rust 2024](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org)
[![Clippy Clean](https://img.shields.io/badge/clippy-0%20warnings-brightgreen.svg)](https://github.com/rust-lang/rust-clippy)
[![Txtar Conformance](https://img.shields.io/badge/txtar%20tests-547%2F547%20passing-brightgreen.svg)](tests/testdata/)
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
│   ├── cue-eval/               # Arena lattice engine, 24 stdlib packages, OpenAPI/JSONSchema importer, manifest
│   ├── cue-derive/             # Procedural macro `#[derive(CueValidate)]` with Serde
│   ├── cue-wasm/               # WebAssembly bindings (`wasm-bindgen`) for browser and Node.js
│   ├── cue-test-harness/       # Upstream .txtar test fixture parser & test runner
│   └── cue-cli/                # CLI binary (`eval`, `vet`, `fmt`, `import`, `mod`, `test-txtar`, `sync-upstream`)
├── tests/
│   └── testdata/               # 547 upstream conformance .txtar suites (527 pass; 499 with --strict-errors)
└── examples/                   # Sample CUE schemas and data files
```

---

## 2. Key Features

- **Lattice Unification ($\sqcap$)**: Greatest lower bound calculation over scalar values, recursive structs, bounds (`>1024 & <65535`), regex constraints (`=~ "^[a-z]+$"`), and closed `#Definitions` (closed where read; `...` opens; see impl/10-closedness.md for the stated divergences).
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

## 3. Usage & Quick Start

### Add to Your Rust Project

Add `cue-eval` and `cue-derive` to your `Cargo.toml`:

```toml
[dependencies]
cue-eval = { git = "https://github.com/tonky/cue-rs" }
cue-derive = { git = "https://github.com/tonky/cue-rs" }
# or when published on crates.io:
# cue-eval = "0.1"
# cue-derive = "0.1"
```

### Rust API Examples

#### 1. Evaluate CUE to JSON / YAML
```rust
use cue_eval::eval_to_json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cue_src = r#"
        #Server: {
            host: string
            port: int & >1024 & <=65535
        }
        prod: #Server & {
            host: "api.example.com"
            port: 8080
        }
    "#;

    let json_val = eval_to_json(cue_src)?;
    println!("{}", serde_json::to_string_pretty(&json_val)?);
    Ok(())
}
```

#### 2. Validate Rust Structs with `#[derive(CueValidate)]`
```rust
use serde::Deserialize;
use cue_derive::CueValidate;

#[derive(Deserialize, CueValidate)]
#[cue(schema = "#User: { id: uint32, name: string & strings.MinRunes(2), email: =~\"@\" }")]
struct User {
    id: u32,
    name: String,
    email: String,
}

fn main() {
    let json_data = serde_json::json!({
        "id": 1,
        "name": "Alice",
        "email": "alice@example.com"
    });

    let user: User = serde_json::from_value(json_data).unwrap();
    user.cue_validate().expect("Schema validation failed");
}
```

#### 3. Validate JSON Directly Against CUE Schemas
```rust
use cue_eval::validate_json;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = "#Config: { timeout_seconds: int & >0 & <=300, replicas: int & >=1 }";
    let data = json!({ "timeout_seconds": 30, "replicas": 3 });

    validate_json(schema, &data)?;
    println!("Payload valid!");
    Ok(())
}
```

---

## 4. CLI Tool (`cue-rs`)

Install the binary:
```bash
cargo install --path crates/cue-cli
```

### Commands

```bash
# 1. Evaluate a CUE file (JSON or YAML format)
cue-rs eval config.cue --format json
cue-rs eval config.cue --format yaml

# 2. Validate (vet) a data file against a schema definition
cue-rs vet schema.cue data.json

# 3. Format CUE source files
cue-rs fmt config.cue --write

# 4. Import JSON Schema or OpenAPI definitions into CUE
cue-rs import json-schema schema.json --root User --write user.cue
cue-rs import openapi petstore.yaml --write api.cue

# 5. Initialize and manage CUE modules
cue-rs mod init example.com/mymod@v0
cue-rs mod tidy

# 6. Run upstream .txtar test suites
cue-rs test-txtar tests/testdata
```

---

## 5. WebAssembly Integration (`cue-wasm`)

Compile to WebAssembly:
```bash
cargo build -p cue-wasm --target wasm32-unknown-unknown --release
```

Or using `wasm-pack`:
```bash
wasm-pack build crates/cue-wasm --target web
```

JavaScript / TypeScript usage:
```javascript
import init, { eval_cue, validate_json, format_cue } from "./pkg/cue_wasm.js";

await init();

// Evaluate CUE code
const jsonStr = eval_cue("a: 10, b: 20, sum: a + b");

// Validate JSON
const isValid = validate_json("#User: { name: string }", JSON.stringify({ name: "Alice" }));

// Format CUE
const formatted = format_cue("x:1\ny:2");
```

---

## 6. Documentation & Roadmap

- [`CUE_CONFORMANCE_TRACKER.md`](CUE_CONFORMANCE_TRACKER.md): Upstream CUE test inventory, feature comparison, and conformance tracking.
- [`CUE_ROADMAP.md`](CUE_ROADMAP.md): Detailed phase breakdown, memory model, and milestone progress.
- [`CUE_RUST_LEARNINGS.md`](CUE_RUST_LEARNINGS.md): Comparative architecture analysis and design trade-offs.
