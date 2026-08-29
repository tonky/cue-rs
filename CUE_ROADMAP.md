# CUE Parser & Evaluator in Rust: Implementation Plan & Tracker

This document tracks the technical design, milestone progress, and conformance verification for the native Rust implementation of the CUE (Configure, Unify, Execute) language engine.

---

## 1. Project Health & Conformance Status

| Metric | Current Status | Target |
| :--- | :--- | :--- |
| **Rust Edition** | **2024** | 2024 |
| **Clippy Lint Status** | **0 warnings (`cargo clippy --workspace --all-targets`)** | 0 warnings |
| **Workspace Crates** | `cue-syntax`, `cue-eval`, `cue-derive`, `cue-test-harness`, `cue-cli` | 5 modular crates |
| **Unit Test Coverage** | **27 / 27 passing (100%)** | 100% |
| **Txtar Fixture Pass Rate** | **74 / 74 passing (100%)** | >95% upstream parity |

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
│                 │ ──► Binary (`0b1100`), Hex (`0x2A`), Octal (`0o755`) & SI Literals (`4Ki`, `10M`, `2G`)
│                 │ ──► Raw & Multi-Line Strings (`#"..."#`, `#"""..."""#`, `#'...'#`)
│                 │ ──► Dynamic Slicing (`items[1:]`, `items[:3]`, `items[2:5]`)
│                 │ ──► Dynamic Selector Chaining on Parenthesized Literals (`({ a: 1 }).a`)
│                 │ ──► Cartesian Multi-Clause List Comprehensions (`[ for x in s1 for y in s2 { ... } ]`)
│                 │ ──► Cartesian Indexed Loop Unpacking (`[ for i, x in s1 for j, y in s2 { ... } ]`)
│                 │ ──► Comprehensions with `let` Bindings & Dynamic Labels (`(cleanKey): v`)
│                 │ ──► Struct Body Comprehensions (`[ for k, v in map { name: k, val: v } ]`)
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
│ (PackageLoader) │ ──► Struct Embedding with Disjunction Schema Selection (`#Prod | #Dev`)
└────────┬────────┘ ──► Module-Aware Package Import Resolution (`import "myorg.com/mod/schema"`)
         │          ──► Inter-Arena Deep Value Cloning (`clone_value_into`)
         │          ──► Optional Field Validation (`field?: type`) & Export Filtering
         │          ──► Hidden Field (`_secret`) & Definition (`#Schema`) Export Filtering
         │          ──► Discriminated Union Struct Disjunctions (`#Circle | #Rectangle`)
         │          ──► Multi-Pass Fixpoint Relaxation Loop for Order-Independent Reference Graphs
         │          ──► Mixed Int/Float Numeric Bounds (`number & >0` matching `12.5` float and `10` int)
         │          ──► Module Discovery (`cue.mod/module.cue` search and `ModuleInfo` parsing)
         │          ──► Multi-Pattern Simultaneous Constraints (`[=~"^STR_"]: string`, `[=~"^NUM_"]: int & >0`)
         │          ──► Nested Dynamic Struct & Array Indexing (`database.environments[env].pool[tier]`)
         │          ──► Lexical Scope Isolation for Nested Struct Blocks
         │          ──► Open List Ellipsis Unification (`[...int]` ⊓ `[1, 2, 3]`)
         │          ──► Hierarchical Type Subsumption (`number` ⊓ `int` ⊓ `uint16` ⊓ `8080`)
         │          ──► Disjunction Meet Algebra (`(A | B) ⊓ (C | D)`)
         │          ──► List Concatenation (`[1, 2] + [3, 4]`) and List/String Repetition (`"x" * 3`, `[0] * 4`)
         │          ──► Multi-File Package Hoisting, Import Aliasing & Cycle Solver
         │          ──► Numeric Type Constraints:
         │                • uint, uint8, uint16, uint32, uint64
         │                • int8, int16, int32, int64, float32, float64
         │          ──► Standard Library Packages (24 packages active):
         │                • strings (MinRunes, MaxRunes, Trim, TrimPrefix, TrimSuffix, Repeat, Replace)
         │                • math (Sqrt, Pow, Log, Sin, Cos, Tan, Asin, Acos, Atan, Atan2, Max, Min, Pi, E, MultipleOf, Floor, Ceil, Round, Trunc, Abs)
         │                • math/bits (And, Or, Xor, Lsh, Rsh, OnesCount)
         │                • list (MinItems, MaxItems, UniqueItems, Sort, FlattenN, Range, Take, Drop, Sum, Product, Avg, Min, Max)
         │                • regexp (Valid, Match, Find, FindAll, ReplaceAll)
         │                • struct (MinFields, MaxFields)
         │                • time (Time RFC3339 validator, Duration parser, Unix, Hour, Minute, Second, Millisecond, Microsecond, Nanosecond)
         │                • net (IPv4, IPv6, IP validators)
         │                • strconv (Atoi, Itoa, ParseFloat, FormatFloat, ParseBool, FormatBool, ParseInt, ParseUint, FormatInt, FormatUint, Quote, Unquote)
         │                • uuid (Valid, Version)
         │                • encoding/json (Marshal, Unmarshal)
         │                • encoding/yaml (Marshal, Unmarshal)
         │                • encoding/html (Escape, Unescape)
         │                • encoding/csv (Decode, Encode)
         │                • encoding/base32 (Encode, Decode)
         │                • encoding/base64 (Encode, Decode, RawURLEncode, RawURLDecode, URLEncode, URLDecode)
         │                • encoding/hex (Encode, Decode)
         │                • text/tabwriter (Write)
         │                • text/template (Execute)
         │                • crypto/sha512 (Sum)
         │                • crypto/sha256 (Sum)
         │                • crypto/md5 (Sum)
         │                • crypto/sha1 (Sum)
         │                • crypto/hmac (SHA512, SHA256, MD5, SHA1)
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
- [x] **Logos Lexer**: Identifiers, Definitions (`#Def`), Hidden fields (`_hidden`), Bottom (`_|_`), Top (`_`), Numbers (`0b`, `0x`, `0o`, SI suffixes), Strings, and Operators.
- [x] **Raw Strings & Multi-Line Literals**: `#""" ... """#`, `#"..."#`, and `#'...'#`.
- [x] **Pratt Expression Parser**: Unification (`&`), Disjunction (`|`), Comparison operators (`==`, `!=`, `<`, `<=`, `>`, `>=`, `=~`, `!~`), Mixed integer/float arithmetic (`+`, `-`, `*`, `/`), Unary arithmetic/bounds, and Selectors/Indexing/Slicing.
- [x] **Parenthesized Selector & Index Chaining**: `({ cluster: { id: "p1" } }).cluster.id` and `(["a", "b"])[1]`.
- [x] **Cartesian & List Comprehensions**: `[ for x in src if x > 1 { x * 10 } ]`, `[ for i, x in s1 for j, y in s2 { ... } ]`, and struct-body mappings `[ for k, v in map { name: k, port: v.port } ]`.
- [x] **Comprehensions with `let` Bindings & Dynamic Labels**: `for k, v in map let uk = strings.ToUpper(k) if strings.HasPrefix(uk, "P_") { (strings.ToLower(uk)): v }`.
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
  - [x] Mixed Int/Float Multi-Constraint Bounds (`number & >0` matching `12.5` and `42`).
  - [x] Regex matching constraints (`=~ "^[a-z]+$"`).
- [x] **Open List Ellipsis Unification**: `[...T]` schema unification with concrete and subtyped lists (`[...int] & [1, 2, 3]`).
- [x] **Optional Field Validation & Export**: `field?: type` constraints applied when present and omitted from export when absent.
- [x] **Hidden Fields & Definitions Export Filtering**: `_internal` fields and `#Definitions` evaluated in scope and filtered from output.
- [x] **Discriminated Union Disjunctions**: Struct disjunction branches matching discriminated tags (`#Circle | #Rectangle`).
- [x] **Disjunction Meet Algebra**: `(A | B) & (C | D)` cross-product branch unification with backtracking.
- [x] **Closed Struct Algebra**: Rejection of unauthorized fields for closed `#Definitions`.
- [x] **Disjunction Pruning**: Branch selection with defaults (`*true | false`).
- [x] **JSON & YAML Serialization**: Direct export of evaluated concrete structs/lists to `serde_json::Value` and YAML.

---

### Phase 3: Advanced Language Features & 24 Standard Library Packages
- [x] **Pattern Constraints on Structs**:
  - [x] Support multiple simultaneous pattern constraints (`[=~"^STR_"]: string`, `[=~"^NUM_"]: int`).
  - [x] Pattern exemption in closed `#Definitions`.
- [x] **Comprehensions**:
  - [x] `for k, v in source { ... }` list and struct iterations.
  - [x] `if condition { ... }` conditional declarations.
  - [x] Chained multi-clause comprehensions (`for x in list if x > 2 if x < 6 { ... }`).
  - [x] `let` local bindings inside comprehension clauses.
  - [x] Cartesian product list comprehensions (`for i, x in src1 for j, y in src2 { ... }`).
  - [x] List comprehensions with stdlib functions in conditions/expressions (`strings.HasPrefix`, `strings.ToUpper`, `strings.Replace`).
  - [x] Dynamic parenthesized label evaluation inside loops `("k_\(i)"): val`.
- [x] **List Indexing & Slicing & Operations**:
  - [x] `list[i]` integer indexing and struct dynamic field indexing (`struct[expr]`).
  - [x] Nested dynamic lookup chains (`database.environments[env].pool[tier]`).
  - [x] `list[low:high]`, `list[low:]`, `list[:high]` range slicing.
  - [x] List concatenation (`l1 + l2`) and repetition (`[0] * 4`).
- [x] **String Interpolation & Repetition**: `"prefix \(expr) suffix"` and `"x" * 10`.
- [x] **24 Standard Library Packages**:
  - [x] `strings`: `MinRunes`, `MaxRunes`, `ToUpper`, `ToLower`, `Contains`, `HasPrefix`, `HasSuffix`, `Join`, `Trim`, `TrimPrefix`, `TrimSuffix`, `Repeat`, `Replace`.
  - [x] `math`: `Sqrt`, `Pow`, `Log`, `Sin`, `Cos`, `Tan`, `Asin`, `Acos`, `Atan`, `Atan2`, `Max`, `Min`, `Pi`, `E`, `MultipleOf`, `Floor`, `Ceil`, `Round`, `Trunc`, `Abs`.
  - [x] `math/bits`: `And`, `Or`, `Xor`, `Lsh`, `Rsh`, `OnesCount`.
  - [x] `list`: `MinItems`, `MaxItems`, `UniqueItems`, `Contains`, `Sort`, `FlattenN`, `Range`, `Take`, `Drop`, `Sum`, `Product`, `Avg`, `Min`, `Max`.
  - [x] `regexp`: `Valid`, `Match`, `Find`, `FindAll`, `ReplaceAll`.
  - [x] `struct`: `MinFields`, `MaxFields`.
  - [x] `time`: `Time` (RFC3339 validator), `Duration` (string duration to nanoseconds), `Unix` (timestamp formatter), `Hour`, `Minute`, `Second`, `Millisecond`, `Microsecond`, `Nanosecond`.
  - [x] `net`: `IPv4`, `IPv6`, `IP` address validators.
  - [x] `strconv`: `Atoi`, `Itoa`, `ParseFloat`, `FormatFloat`, `ParseBool`, `FormatBool`, `ParseInt`, `ParseUint`, `FormatInt`, `FormatUint`, `Quote`, `Unquote`.
  - [x] `uuid`: `Valid`, `Version`.
  - [x] `encoding/json`: `Marshal`, `Unmarshal`.
  - [x] `encoding/yaml`: `Marshal`, `Unmarshal`.
  - [x] `encoding/html`: `Escape`, `Unescape`.
  - [x] `encoding/csv`: `Decode`, `Encode`.
  - [x] `encoding/base32`: `Encode`, `Decode`.
  - [x] `encoding/base64`: `Encode`, `Decode`, `RawURLEncode`, `RawURLDecode`, `URLEncode`, `URLDecode`.
  - [x] `encoding/hex`: `Encode`, `Decode`.
  - [x] `text/tabwriter`: `Write`.
  - [x] `text/template`: `Execute`.
  - [x] `crypto/sha512`: `Sum`.
  - [x] `crypto/sha256`: `Sum`.
  - [x] `crypto/md5`: `Sum`.
  - [x] `crypto/sha1`: `Sum`.
  - [x] `crypto/hmac`: `SHA512`, `SHA256`, `MD5`, `SHA1`.
  - [x] `path`: `Base`, `Dir`, `Ext`, `Join`.

---

### Phase 4: Scoping, Multi-file Packages & Rust Procedural Macro
- [x] **Struct Embedding with Disjunction Selection**: Embedding `#Disjunction` schemas into target structs (`{ #ProdConfig | #DevConfig, name: "app" }`).
- [x] **Lexical Scope Isolation**: Nested struct evaluations maintain private environments with outermost scope resolution.
- [x] **Multi-Pass Reference Relaxation**: Fixpoint loop for forward and mutually derived struct fields (`a: b + 1, b: c * 2, c: 10`).
- [x] **Multi-File Package Loader (`PackageLoader`)**: Evaluates all `.cue` files in a directory as a unified package environment with cross-file definition hoisting and import aliasing.
- [x] **Module Root Discovery (`cue.mod/module.cue`)**: Upward directory traversal extracting module name and language version into `ModuleInfo`.
- [x] **Module-Aware Package Import Resolution**: Seamless resolution and evaluation of module packages (`import "myorg.com/app/schema"`) and vendored packages (`cue.mod/pkg/...`).
- [x] **Inter-Arena Deep Value Cloning (`clone_value_into`)**: Recursive value allocation across independent package evaluation arenas.
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
