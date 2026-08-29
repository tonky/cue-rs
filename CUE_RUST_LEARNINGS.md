# Architectural Learnings & Design Trade-offs: Native CUE in Rust

This document synthesizes key technical insights, comparative architecture analysis (including against `tyrchen/cue-rust` and upstream Go `cue-lang/cue`), and design decisions for building a high-performance CUE engine in pure Rust.

---

## 1. Memory Model & Graph Representation

### A. Arena (`SlotMap<ValueId, Value>`) vs. Recursive ADT Tree
* **Go CUE Approach**: Go uses garbage-collected heap pointer graphs (`*adt.Vertex`, `*adt.Node`). In Go, circular references during cycle detection are naturally supported by GC pointer cycles.
* **Pure Rust ADT Tree (`tyrchen/cue-rust`)**: Lowering AST into recursive ADT enums works well for tree structures, but recursive graphs with cyclic schema references (`#Tree: { left?: #Tree }`) or forward references require `Rc<RefCell<...>>` or `Arc<RwLock<...>>`, introducing runtime overhead and potential cycle memory leaks.
* **Our Arena Model (`cue-rs`)**:
  * All values live in a centralized `SlotMap<ValueId, Value>` arena.
  * References between nodes are compact, copyable `ValueId` handles (generational index).
  * **Zero RefCell / Arc overhead** during graph traversal and lattice meet ($\sqcap$) operations.
  * Simplifies two-pass definition hoisting and circular graph handling without memory leaks.

---

## 2. Disjunctions & Transactional Backtracking

### The Lattice Meet Challenge
In CUE, disjunctions `A | B` unified with other constraints `(A | B) & (C | D)` require exploring branches that might fail ($\bot$). If a branch partially modifies a struct or allocates dependent values before failing, those changes must not contaminate subsequent branches.

### Design Trade-offs
1. **Full Deep Clone**: Cloning the entire graph on each disjunctive branch is expensive in memory and CPU ($O(N)$ copies).
2. **Transactional Backtracking Trail (Implemented)**:
   * Record allocations and state changes in a lightweight trail stack.
   * `checkpoint()` before testing a disjunct; `rollback()` if evaluation evaluates to bottom ($\bot$).
   * Enables branch exploration with minimal overhead ($O(1)$ checkpointing).

---

## 3. Order-Independent Evaluation: Multi-Pass Fixpoint Relaxation

### The Problem
CUE allows fields to reference other fields regardless of declaration order, including mutual derivations:
```cue
a: b + 1
b: c * 2
c: 10
```

### Techniques Compared
* **Topological Sort Compiler**: Requires static dependency analysis before evaluation. Fails or complicates dynamic expressions like `items[currentEnv]` where dependencies are dynamically computed.
* **Multi-Pass Fixpoint Loop (Implemented)**:
  * Iteratively resolves unresolved references until no new values can be derived (fixpoint reached).
  * True unresolvable cycles (e.g. `a: b + 1, b: a + 1`) terminate in bounded iterations ($N$ fields) and are deterministically flagged as bottom ($\bot$).

---

## 4. Rust-First Ecosystem & Schema Interoperability

### Procedural Macros vs. Runtime SDK
* While standard SDK bindings (`compile_source`, `lookup`) are essential for dynamic workflows, Rust applications benefit most from **zero-boilerplate compile-time integration**.
* With `cue-derive`, Rust structs get validation hooks via `serde`:
  ```rust
  #[derive(Deserialize, CueValidate)]
  #[cue(schema = "#Config: { port: uint16 & >1024, host: string }")]
  struct Config {
      port: u16,
      host: String,
  }
  ```
* This pattern makes CUE a drop-in replacement for hand-written serde validators and JSONSchema validators in Rust backends and AI agent runtimes.

---

## 5. Pure Rust Standard Library Packages

### Zero-Dependency Portability
* Upstream CUE relies on the Go standard library (`crypto/*`, `encoding/*`, `net`, `strconv`, `time`, `text/template`).
* Rather than binding to external C libraries (OpenSSL, etc.) or spawning external processes, implementing pure Rust zero-dependency versions of the 23 stdlib packages ensures:
  1. Instant compilation.
  2. Total sandbox safety (can run in restricted WASM or microVM runtimes).
  3. Consistent cross-platform behavior across macOS, Linux, and Windows.

---

## 6. Upstream Test-Driven Conformance (`.txtar`)

* The `.txtar` format used by Go's testing toolchain is the gold standard for CUE language conformance.
* By building a dedicated `.txtar` parser and test runner (`cue-test-harness`), every language feature added (slicing, raw strings, binary literals, pattern constraints, stdlib functions) is immediately verified against multi-file test fixtures.
