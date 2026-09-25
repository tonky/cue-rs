# Closed definitions

Started from enact's Cleanup B. enact wanted `deny_unknown_fields`, and its
pipelines unify with `#Pipeline`, but a misspelled key passed CUE silently.
cue-rs had a closedness flag that nothing ever set.

## Method

- `cue export` v0.16.1 as the oracle, over 31 probes and then 10 more for
  embedding variants.
- The harness gained `--strict-errors` first, so the corpus could say which
  "expected error" cases were real passes. It reported 65 failures on the
  evaluator before this phase and 48 after, with none new.

## Decisions

- **Close where read, not where declared.** Closing at declaration would close
  the definition's own body while it is still being built (`#B: {#A, y: int}`).
  The closed copy is memoised per value id and cloned, never mutated. A
  definition's id is shared by every reader.
- **Embedding opens, then recloses.** This is simpler than tracking which
  literal admitted which field, and it gives upstream's verdict on all 10
  embedding probes.
- **File root exemption.** Three corpus cases regressed until the file root and
  the definitions read while evaluating a root embedding stayed open. That is
  an approximation; see follow_up.md.
- **Union of fields on merge.** Accepted by the user for now. Upstream's
  per-conjunct `closeInfo` is its own phase.

## The originDir detour

enve's `distributed_monorepo` example broke under closedness. The cause was
cue-eval's `attach_origin_dir_to_structs`, which injected `originDir` into a
closed `healthCheck`. The failure surfaced as "reference backendPkg not found",
because the import's load error was discarded. Both are fixed here:
- the injection is replaced by an opt-in, marker-based annotation;
- import errors propagate.

Once the marker was on `#Service`, a real enve service still got no
`originDir`. The schema declares `originDir?: string`, and the annotation
treated a declared field as set. The old heuristic had the same check, so
`#Service`-typed services had never received `originDir` at all. Only untyped
structs that happened to carry `command` or `image` did.

## Numbers

- 146 workspace tests. Corpus 20 failures (was 21), strict 48 (was 65).
- Cost 0.54 s / 202 MB against HEAD's 0.54 s / 202 MB.
- enve 737 green with the patched pin.
