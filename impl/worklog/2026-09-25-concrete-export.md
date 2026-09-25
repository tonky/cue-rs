# Concrete export compatibility

Implemented the first phase-13 family under the approved phases 12-15 roadmap.
Moved concrete traversal from Evaluator into `export`; both evaluator exports
and builtin JSON/YAML encoders now use the same default and optional-field
rules. Function calls no longer pick the first of several marked defaults.

Exact integer tokens stay JSON numbers through export and JSON decode. Bytes
export as base64. The AST now models bytes as octets: `\xff` is byte FF, distinct
from Unicode `\u00ff`. Formatting/reparsing retains those bytes. A borrowed
Serde adapter sends numeric primitives to the YAML serializer, avoiding the
private arbitrary-precision JSON Number protocol. Huge numbers beyond the
serializer's exact range use JSON flow syntax (valid YAML 1.2).

Investigated open lists against pinned upstream: exporting their known prefix
is correct and is preserved. Math results outside f64's finite range now report
an error rather than JSON null; general decimal arithmetic remains unsupported.
Float string presentation and enormous-number YAML layout remain separate gaps.

Validation: 196 ordinary workspace tests pass, three real-reference integration
tests explicitly pass (199 unique tests total); Clippy clean. The reduced
reference fixtures are in tests/conformance/regressions/export and run through
the pinned Go adapter. Full corpus: two newly passed checks, none regressed,
517 passed / 1095 mismatch / 3269 unsupported / 1 oracle disagreement /
603 not applicable. All 547 originals unchanged; only reviewed baseline
outcomes updated. The mismatch count is across 200 archives, not a bug count.

Odoo release outputs match saved upstream JSON for refs/literal; original
rejects normally. RSS effectively unchanged: 349288, 577436, 2185604 KiB
respectively; times 0.68, 1.16, 4.04 s. Every build/evaluation/test ran under
process-tree caps; 3 GiB used only for the known original baseline.

Workspace CLI/wasm YAML consumers migrated. Documented enve's corresponding
`export_formatted` change but did not alter its dirty checkout or revision pins.
Phase 13 remains open; the next memory step starts against this checked baseline.
