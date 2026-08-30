# CUE in Rust (2024 Edition) — System Architecture & Source Map

This document serves as the canonical architectural guide, module locator index, and reference map for both human developers and autonomous AI coding agents collaborating on `cue-rs`.

---

## 1. Workspace Dependency Graph

```
┌──────────────┐     ┌──────────────┐
│  cue-syntax  │ ──► │   cue-eval   │ ──► ┌──────────────┐
└──────────────┘     └──────────────┘     │  cue-derive  │
       │                    │             └──────────────┘
       │                    ▼                    │
       └──────────────► ┌──────────────┐ ◄───────┘
                        │   cue-cli    │
                        └──────────────┘
                               ▲
                        ┌──────────────┐
                        │cue-test-harn.│
                        └──────────────┘
```

| Crate | Path | Responsibility | Primary Types & Exports |
| :--- | :--- | :--- | :--- |
| **`cue-syntax`** | `crates/cue-syntax` | Lexer, Pratt Parser, Automatic Semicolon Insertion (ASI), AST, AST Pretty-Printer / Formatter | `Token`, `Expr`, `Decl`, `SourceFile`, `parse_file`, `format_file` |
| **`cue-eval`** | `crates/cue-eval` | SlotMap Arena memory model, Lattice Unification ($\sqcap$), Multi-pass Relaxation Evaluator, Package Loader, 13 Modular Stdlib Packages | `Evaluator`, `ValueArena`, `ValueId`, `Value`, `unify`, `PackageLoader`, `call_stdlib_func` |
| **`cue-derive`** | `crates/cue-derive` | Procedural macro `#[derive(CueValidate)]` with serde schema validation | `CueValidate` derive macro |
| **`cue-test-harness`**| `crates/cue-test-harness`| Upstream CUE `.txtar` test fixture parser and executor | `TxtarArchive`, `TxtarFile` |
| **`cue-cli`** | `crates/cue-cli` | CLI binary `cue-rs` (`eval`, `vet`, `fmt`, `test-txtar`, `sync-upstream`) | `main()`, CLI subcommands |

---

## 2. Evaluation & Unification Pipeline

```
[ CUE Source (.cue) ]
         │
         ▼
[ Logos Lexer (token.rs) ] ──► Token Stream + Balanced Attribute Parsing (@tag)
         │
         ▼
[ Pratt Parser (parser.rs) ] ──► Automatic Semicolon Insertion (ASI) + Label Sugar
         │
         ▼
[ AST: SourceFile (ast.rs) ] ──► Decls, Expressions, Comprehensions, Disjunctions
         │
         ▼
[ Evaluator (eval.rs) ] ──► Scope Stack + Multi-Pass Relaxation Fixed-Point Loop
         │
         ├──► [ SlotMap Arena (value.rs) ] ──► Generational ValueId Node Allocations
         │
         ├──► [ Unifier (unify.rs) ] ──► Lattice Meet (⊓) + Transactional Backtrack Trail
         │
         └──► [ Modular Stdlib (stdlib/) ] ──► 13 domain-specific built-in packages
         │
         ▼
[ Serializer / JSON Exporter ] ──► to_json() / CLI Output / Serde Validation Result
```

---

## 3. Detailed Source Map & Feature Locator

### A. Syntax, Parsing, & Formatting (`crates/cue-syntax/src/`)
- [`token.rs`](crates/cue-syntax/src/token.rs): Logos token definitions, string literal regexes, raw string literals (`#"..."#`, `#'...'#`), multiline strings (`"""`), custom attribute lexer (`@tag(...)`) with balanced parentheses, tilde operator (`~`), and token display.
- [`parser.rs`](crates/cue-syntax/src/parser.rs): Pratt recursive descent expression parser, ASI virtual comma insertion, dynamic label lookaheads (`(key):` and `[pattern]~(alias):`), Cartesian list comprehensions, `let` bindings, disjunction defaults (`*T | U`).
- [`ast.rs`](crates/cue-syntax/src/ast.rs): AST nodes (`SourceFile`, `Decl`, `FieldDecl`, `Label`, `Expr`, `ListLit`, `StructLit`, `ComprehensionDecl`).
- [`formatter.rs`](crates/cue-syntax/src/formatter.rs): Canonical pretty-printer formatting AST back into idiomatic CUE source code.

### B. Evaluator, Arena, & Unification (`crates/cue-eval/src/`)
- [`value.rs`](crates/cue-eval/src/value.rs): Generational `SlotMap` arena (`ValueArena`), `ValueId`, `Value` enum variants (Bottom, Top, Null, Bool, Int, Float, String, Bytes, Struct, List, Bounds, Validators, RecursiveRef), and transactional rollback checkpoints (`checkpoint()` / `rollback()`).
- [`eval.rs`](crates/cue-eval/src/eval.rs): Evaluator state, lexical environment scopes (`push_scope()` / `pop_scope()`), multi-pass relaxation loop for forward/order-independent references, top-level builtins (`len()`, `close()`), expression evaluation, and JSON serialization (`to_json()`).
- [`unify.rs`](crates/cue-eval/src/unify.rs): Greatest Lower Bound lattice meet ($\sqcap$) algorithm, scalar unification, recursive struct merging, closed struct enforcement, disjunction selection, regex validation (`=~`), and bound checking (`>10 & <100`).
- [`package.rs`](crates/cue-eval/src/package.rs): `PackageLoader`, `cue.mod/module.cue` discovery, multi-file directory aggregation, module-aware package import resolution, and inter-arena value deep copying.

### C. Modular Standard Library (`crates/cue-eval/src/stdlib/`)
- [`stdlib/mod.rs`](crates/cue-eval/src/stdlib/mod.rs): Root router `call_stdlib_func`, `StdlibValidator` enum, and validator dispatch.
- [`stdlib/strings.rs`](crates/cue-eval/src/stdlib/strings.rs): `strings.*` (Split, Join, Contains, HasPrefix, HasSuffix, ReplaceAll, Trim, TrimPrefixAny, Runes, Title, etc.).
- [`stdlib/math.rs`](crates/cue-eval/src/stdlib/math.rs): `math.*` & `math/bits` (constants `Pi`, `E`, `Phi`, trigonometric functions, `Floor`, `Ceil`, `Round`, `FMA`, `Pow10`, `Frexp`, `Modf`, `Erf`, `Gamma`, `Nextafter`, `MultipleOf`).
- [`stdlib/list.rs`](crates/cue-eval/src/stdlib/list.rs): `list.*` (Concat, Flatten, Drop, Take, Slice, Sort, SortStrings, IsSorted, Chunk, Distinct, Zip, Unzip, UniqueItems).
- [`stdlib/crypto.rs`](crates/cue-eval/src/stdlib/crypto.rs): `crypto/*` (`sha256.Sum256`, `sha512.Sum512`, `md5.Sum`, `sha1.Sum`, `hmac.Sign` with hash constants).
- [`stdlib/encoding.rs`](crates/cue-eval/src/stdlib/encoding.rs): `encoding/*` (`json`, `yaml`, `csv`, `html`, `toml`, `base64`, `base32`, `hex`).
- [`stdlib/time.rs`](crates/cue-eval/src/stdlib/time.rs): `time.*` (RFC3339 validation, `ParseDuration`, `FormatDuration`, `Unix`, `Year`, `Month`, `Day`, `Parse`).
- [`stdlib/net.rs`](crates/cue-eval/src/stdlib/net.rs): `net.*` (IPv4, IPv6, IP, `ParseIP`, `ParseCIDR`, `IPInCIDR`, `FQDN`, `SplitHostPort`, `JoinHostPort`).
- [`stdlib/path.rs`](crates/cue-eval/src/stdlib/path.rs): `path.*` & `path/filepath` (`Clean`, `Join`, `Split`, `Dir`, `Base`, `Ext`, `Match`).
- [`stdlib/regexp.rs`](crates/cue-eval/src/stdlib/regexp.rs): `regexp.*` (`Valid`, `Find`, `FindAll`, `FindSubmatch`, `QuoteMeta`, `ReplaceAll`).
- [`stdlib/struct_pkg.rs`](crates/cue-eval/src/stdlib/struct_pkg.rs): `struct.*` (`MinFields`, `MaxFields`).
- [`stdlib/strconv.rs`](crates/cue-eval/src/stdlib/strconv.rs): `strconv.*` (`Atoi`, `FormatInt`, `FormatFloat`, `ParseInt`, `ParseFloat`, `Quote`).
- [`stdlib/uuid.rs`](crates/cue-eval/src/stdlib/uuid.rs): `uuid.*` (`Valid`, `Version`, `URN`).
- [`stdlib/tabwriter.rs`](crates/cue-eval/src/stdlib/tabwriter.rs): `text/tabwriter.*` and `text/template.*`.

---

## 4. Key Invariants & Safety Guarantees

1. **Arena Memory Model**:
   - Nodes are managed in `ValueArena` using generational `slotmap::SlotMap<ValueId, Value>`.
   - References are stored as `ValueId` handles (never raw Rust references or raw pointers), completely preventing use-after-free, memory corruption, and cyclic reference leaks.
2. **Transactional Backtracking**:
   - The arena maintains a `trail: Vec<ValueId>`.
   - Speculative unifications (e.g. testing disjunction branches) use `checkpoint()` and `rollback()` to cleanly revert all allocated nodes and state if a branch produces $\bot$ (bottom).
3. **Rust 2024 Idioms**:
   - Strict compile-time checks with **0 Clippy warnings** (`cargo clippy --workspace --all-targets`).
   - Pure, safe Rust (no `unsafe` blocks).
   - Let-chain matching (`if let Some(...) && let Some(...)`).

---

## 5. Development & Verification Workflows

```bash
# 1. Run all workspace unit and integration tests (27+ tests)
cargo test --workspace

# 2. Run all conformance txtar test suites (158+ tests)
cargo run -p cue-cli -- test-txtar tests/testdata

# 3. Check for 0 Clippy warnings
cargo clippy --workspace --all-targets

# 4. Sync additional upstream test suites from a local CUE repo checkout
cargo run -p cue-cli -- sync-upstream --src /Users/tonky/projects/cue/cue/testdata/resolve
```
