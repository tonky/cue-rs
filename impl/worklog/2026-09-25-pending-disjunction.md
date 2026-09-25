# A disjunction whose branches are pending stays pending

Found by enact typing the n8n overlay with `schema.#Pipeline`. A component's
`services?: {[string]: _} | [..._]` met `[n8n.services.postgres]`, with `n8n`
declared in another file of the package, and the load failed with
`no matching disjunction branch: [...; reference "n8n" not found]`. Upstream
exports it.

## Cause

Forward references resolve through the relaxation loop: a bottom whose kind
`may_resolve_later` is evaluated again on the next pass. The minimal case is
one file, `c: (int | [..._]) & [top.pg]` with `top` declared after `c`. The
list branch failed because `top` was not evaluated yet. `unify_disjunction_inner`
then reported "no matching disjunction branch" as a `Conflict`, which is final,
so the loop never came back. Structs did not hit it because a struct literal's
fields stay pending on their own.

## Fix

`settle_disjunction` builds the result for both paths of
`unify_disjunction_inner`. When no branch survived and any branch failed with a
kind that may resolve later, the result is `Unresolved` with the same message.
A disjunction that stays pending past the final pass still fails with that
message.

`tests/pending_disjunction.rs` covers:
- forward references in one file, including a disjunction on both sides and a
  definition's field;
- a reference to another file of the package;
- disjunctions that still fail: resolved, forward, and never resolved.

Every expectation was checked against `cue export` v0.16.1. Two of the three
tests fail without the fix.

## Results

- 150 workspace tests. Clippy and fmt are clean.
- Corpus unchanged: 527/547.
- Strict harness: 48 → 49 failures. The new one, `disjselfcycle`, passed by
  accident before: it failed on an error upstream does not report, and the
  harness counted that as the recorded one. Details in `follow_up.md`.

## Left open

A pending branch is still dropped when another branch survives
(`follow_up.md`).
