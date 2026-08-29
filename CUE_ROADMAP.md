# CUE Parser & Evaluator in Rust: Implementation Plan & Tracker

This document tracks the technical design, milestone progress, and conformance verification for the native Rust implementation of the CUE (Configure, Unify, Execute) language engine.

---

## 1. Project Health & Conformance Status

| Metric | Current Status | Target |
| :--- | :--- | :--- |
| **Rust Edition** | **2024** | 2024 |
| **Clippy Lint Status** | **0 warnings (`cargo clippy --workspace --all-targets`)** | 0 warnings |
| **Workspace Crates** | `cue-syntax`, `cue-eval`, `cue-derive`, `cue-test-harness`, `cue-cli` | 5 modular crates |
| **Unit Test Coverage** | **25 / 25 passing (100%)** | 100% |
| **Txtar Fixture Pass Rate** | **41 / 41 passing (100%)** | >95% upstream parity |

---

## 2. Architectural Design

```
[ CUE Source (.cue) ]
         │
         ▼
┌─────────────────┐
│   cue-syntax    │ ──► Logos Lexer + Automatic Semicolon Insertion (ASI)
│   (AST & CST)   │ ──► Pratt Recursive Descent Parser + String Interpolation
│   (Formatter)   │ ──► AST Pretty-Printer / Formatter (`cue-rs fmt`)
│                 │ ──► List Comprehensions (`[ for x in src if x > 1 { x * 10 } ]`)
│                 │ ──► Field Attributes Parser (`@protobuf`, `@json`, `@tag`)
│                 │ ──► Single & Multi-Import Statements (`import s "strings"`)
│                 │ ──► Dynamic & Interpolated Field Labels (`(key): val`, `"\(k)_env": val`)
│                 │ ──► Field Aliases & Let Bindings (`let X = expr`, `A=field: 1`)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    cue-eval     │ ──► SlotMap Arena Allocation with Transactional Trail
│ (Lattice Engine)│ ──► Greatest Lower Bound Unification (⊓) with Disjunction Rollback
│ (PackageLoader) │ ──► Multi-Pass Fixpoint Relaxation Loop for Order-Independent Reference Graphs
└────────┬────────┘ ──► Dynamic Struct Indexing (`ports[env]`) & Nested Selector Chains
         │          ──► Lexical Scope Isolation for Nested Struct Blocks
         │          ──► Open List Ellipsis Unification (`[...int]` ⊓ `[1, 2, 3]`)
         │          ──► Hierarchical Type Subsumption (`number` ⊓ `int` ⊓ `uint16` ⊓ `8080`)
         │          ──► Disjunction Meet Algebra (`(A | B) ⊓ (C | D)`)
         │          ──► List Concatenation (`[1, 2] + [3, 4]`) and List/String Repetition (`"x" * 3`, `[0] * 4`)
         │          ──► Multi-File Package Hoisting, Import Aliasing & Cycle Solver
         │          ──► Numeric Type Constraints:
         │                • uint, uint8, uint16, uint32, uint64
         │                • int8, int16, int32, int64, float32, float64
         │          ──► Standard Library Packages (14 packages active):
         │                • strings (MinRunes, MaxRunes, Trim, TrimPrefix, Repeat)
         │                • math (Sqrt, Pow, Log, Sin, Cos, Max, Min, Pi, E, MultipleOf, Floor, Ceil, Round, Abs)
         │                • list (MinItems, MaxItems, UniqueItems, Sort, FlattenN, Range, Take, Drop)
         │                • regexp (Valid, Match, Find, FindAll, ReplaceAll)
         │                • struct (MinFields, MaxFields)
         │                • time (Time RFC3339 validator, Duration parser)
         │                • net (IPv4, IPv6, IP validators)
         │                • strconv (Atoi, Itoa, ParseFloat, FormatFloat)
         │                • uuid (Valid, Version)
         │                • encoding/json (Marshal, Unmarshal)
         │                • encoding/yaml (Marshal, Unmarshal)
         │                • encoding/base64 (Encode, Decode)
         │                • encoding/hex (Encode, Decode)
         │                • crypto/sha256 (Sum)
         │                • path (Base, Dir, Ext, Join)
         ▼
┌─────────────────┐
│   cue-derive    │ ──► Rust Proc-Macro `#[derive(CueValidate)]` with Serde
└────────┬────────┘
         ▼
┌─────────────────┐
│     Exports     │ ──► JSON and YAML Serialization (`cue-rs eval --format yaml`)
└─────────────────┘ ──► Serde `validate_json` API
```

---

## 3. Milestone Tracker & Roadmap

### Phase 1: Lexer, Parser, Formatter & Test Harness
- [x] **Logos Lexer**: Identifiers, Definitions (`#Def`), Hidden fields (`_hidden`), Bottom (`_|_`), Top (`_`), Numbers, Strings, and Operators.
- [x] **Pratt Expression Parser**: Unification (`&`), Disjunction (`|`), Comparison operators (`==`, `!=`, `<`, `<=`, `>`, `>=`, `=~`, `!~`), Mixed integer/float arithmetic (`+`, `-`, `*`, `/`), Unary arithmetic/bounds, and Selectors/Indexing.
- [x] **List Comprehensions**: `[ for x in src if x > 1 { x * 10 } ]` and `[ for i, x in src { i + x } ]`.
- [x] **Import Declarations & Aliases**: Single and multi-import blocks (`import ( s "strings", json "encoding/json" )`).
- [x] **Dynamic & Interpolated Field Labels**: `(expr): val` and `"\(expr)_suffix": val`.
- [x] **Field Aliases & Let Bindings**: `let Identifier = Expr` and `Alias = Expr`.
- [x] **Field Attributes (`@tag`)**: Parsing `@protobuf(1, int64)` and `@json(name)` annotations into AST.
- [x] **Automatic Semicolon / Statement Insertion (ASI)**: Newline-aware statement separator insertion to prevent multi-line parsing ambiguities.
- [x] **Code Formatter (`cue-rs fmt`)**: AST pretty-printer formatting CUE source files with indentation, list comprehensions, field attributes, and canonical operator spacing.
- [x] **Txtar Test Runner**: Full parser for Go `.txtar` test fixtures to enable test-driven development against upstream test cases.
- [x] **CLI Tool (`cue-rs`)**: `eval` (with `--format json/yaml`), `vet` (schema validation), `fmt` (code formatting), and `test-txtar` commands.

---

### Phase 2: Lattice Values & Unification Engine
- [x] **Arena Memory Model**: `slotmap::SlotMap<ValueId, Value>` avoiding borrow-checker cycles.
- [x] **Lattice Meet Operation ($\sqcap$)**:
  - [x] Scalar values ($v \sqcap v = v$, conflicts yield $\bot$).
  - [x] Type vs. Concrete instances (`int & 42 -> 42`, `string & 42 -> _|_`).
  - [x] Fixed-width numeric types (`uint`, `uint8`, `uint16`, `uint32`, `uint64`, `int8`, `int16`, `int32`, `int64`, `float32`, `float64`).
  - [x] Hierarchical Type Subsumption (`number & int & uint16 & 8080` $\to$ `8080`, `number & float & float64 & 3.14` $\to$ `3.14`).
  - [x] Multi-constraint Bounds (`int & >1024 & <65535`).
  - [x] Regex matching constraints (`=~ "^[a-z]+$"`).
- [x] **Open List Ellipsis Unification**: `[...T]` schema unification with concrete and subtyped lists (`[...int] & [1, 2, 3]`).
- [x] **Disjunction Meet Algebra**: `(A | B) & (C | D)` cross-product branch unification with backtracking.
- [x] **Closed Struct Algebra**: Rejection of unauthorized fields for closed `#Definitions`.
- [x] **Disjunction Pruning**: Branch selection with defaults (`*true | false`).
- [x] **JSON & YAML Serialization**: Direct export of evaluated concrete structs/lists to `serde_json::Value` and YAML.

---

### Phase 3: Advanced Language Features & 14 Standard Library Packages
- [x] **Pattern Constraints on Structs**:
  - [x] Support `[Expr]: Type` constraint evaluation (e.g. `[=~"^app\\.kubernetes\\.io/"]: string`).
  - [x] Pattern exemption in closed `#Definitions`.
- [x] **Comprehensions**:
  - [x] `for k, v in source { ... }` list and struct iterations.
  - [x] `if condition { ... }` conditional declarations.
  - [x] Chained multi-clause comprehensions (`for x in list if x > 2 if x < 6 { ... }`).
  - [x] List comprehensions producing evaluated lists (`[ for x in raw if x > 2 { x * 10 } ]`).
  - [x] Dynamic parenthesized label evaluation `(expr): value`.
- [x] **List Indexing & Slicing & Operations**:
  - [x] `list[i]` integer indexing and struct dynamic field indexing (`struct[expr]`).
  - [x] `list[low:high]` range slicing.
  - [x] List concatenation (`l1 + l2`) and repetition (`[0] * 4`).
- [x] **String Interpolation & Repetition**: `"prefix \(expr) suffix"` and `"x" * 10`.
- [x] **14 Standard Library Packages**:
  - [x] `strings`: `MinRunes`, `MaxRunes`, `ToUpper`, `ToLower`, `Contains`, `HasPrefix`, `HasSuffix`, `Join`, `Trim`, `TrimPrefix`, `TrimSuffix`, `Repeat`.
  - [x] `math`: `Sqrt`, `Pow`, `Log`, `Sin`, `Cos`, `Max`, `Min`, `Pi`, `E`, `MultipleOf`, `Floor`, `Ceil`, `Round`, `Abs`.
  - [x] `list`: `MinItems`, `MaxItems`, `UniqueItems`, `Contains`, `Sort`, `FlattenN`, `Range`, `Take`, `Drop`.
  - [x] `regexp`: `Valid`, `Match`, `Find`, `FindAll`, `ReplaceAll`.
  - [x] `struct`: `MinFields`, `MaxFields`.
  - [x] `time`: `Time` (RFC3339 validator), `Duration` (string duration to nanoseconds).
  - [x] `net`: `IPv4`, `IPv6`, `IP` address validators.
  - [x] `strconv`: `Atoi`, `Itoa`, `ParseFloat`, `FormatFloat`.
  - [x] `uuid`: `Valid`, `Version`.
  - [x] `encoding/json`: `Marshal`, `Unmarshal`.
  - [x] `encoding/yaml`: `Marshal`, `Unmarshal`.
  - [x] `encoding/base64`: `Encode`, `Decode`.
  - [x] `encoding/hex`: `Encode`, `Decode`.
  - [x] `crypto/sha256`: `Sum`.
  - [x] `path`: `Base`, `Dir`, `Ext`, `Join`.

---

### Phase 4: Scoping, Multi-file Packages & Rust Procedural Macro
- [x] **Struct Embedding**: Embedding `#Definitions` and structs into target structs (`{ #Base, extra: 1 }`).
- [x] **Lexical Scope Isolation**: Nested struct evaluations maintain private environments with outermost scope resolution.
- [x] **Multi-Pass Reference Relaxation**: Fixpoint loop for forward and mutually derived struct fields (`a: b + 1, b: c * 2, c: 10`).
- [x] **Multi-File Package Loader (`PackageLoader`)**: Evaluates all `.cue` files in a directory as a unified package environment with cross-file definition hoisting and import aliasing.
- [x] **Serde Direct Validation API**: `cue_eval::validate_json(&schema_str, &json_data) -> Result<(), EvalError>`.
- [x] **Rust Procedural Macro (`cue-derive`)**: `#[derive(CueValidate)]` with `#[cue(schema = "...")]` or `#[cue(file = "...")]`.

---

### Phase 5: Deep Graph Unification & Fixed-Point Cycle Solver
- [x] **Disjunction Backtracking Trail**: Transactional `checkpoint()` and `rollback()` in `ValueArena` preventing memory leak upon exploring failing disjunctive branches.
- [x] **Two-Pass Definition Hoisting**: Pre-registration of `#Definitions` allowing mutual and self-referencing schemas.
- [x] **Recursive Schema Graph Solver**: Self-referential schema unification (e.g. `#Tree: { name: string, left?: #Tree, right?: #Tree }`).
- [x] **Cycle Detection**: Unresolvable value dependency cycles are detected and produce descriptive bottom errors.

---

### Phase 6: Ecosystem & Tooling (Postponed per User Direction)
- [ ] **WASM Target**: Compile `cue-eval` to WebAssembly (`wasm32-unknown-unknown` / `wasm-bindgen`).
- [ ] **OpenAPI / JSONSchema Importer**: Converting JSON Schema & OpenAPI v3 specs into CUE `#Definitions`.
- [ ] **Module Management & OCI Registry**: `cue.mod/module.cue` parser and OCI artifact fetching.
