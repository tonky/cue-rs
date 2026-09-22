//! A reference to a field with a disjunction default must see the value unification
//! selected, not the default.
//!
//! `{p: int | *5, c: "v\(p)"} & {p: 9}` exports `p: 9` beside `c: "v5"`: the field itself
//! takes the override, but everything that read it kept the default. Upstream `cue`
//! v0.16.1 gives `c: "v9"` for every case below; each expectation here is that engine's
//! output.
//!
//! Found from `enve`, where the shape is every service preset in `pkgs/services.cue`:
//!
//! ```cue
//! #PostgresService: {
//!     port: #Port | *5432
//!     command: string | *"postgres -p \(port)"
//!     environment: {PGPORT: "\(port)", DATABASE_URL: "postgresql://\(user)@localhost:\(port)/"}
//! }
//! postgres: #PostgresService & {port: 15432}
//! ```
//!
//! That exports `port: 15432` alongside `DATABASE_URL=...localhost:5432/...`, so the
//! server listens on one port and the application is handed another. Overriding a default
//! is the only reason to write such a preset, so the defect reaches a user as a connection
//! refused rather than as an evaluation error.

use cue_eval::{PackageLoader, eval_to_json};
use serde_json::json;
use std::path::Path;

/// The smallest form: one struct literal, one override, no definition and no reference.
#[test]
fn an_overridden_default_is_seen_by_an_interpolation_in_the_same_struct() {
    assert_eq!(
        eval_to_json(r#"out: {p: int | *5, c: "v\(p)"} & {p: 9}"#).unwrap(),
        json!({"out": {"p": 9, "c": "v9"}})
    );
}

/// Unification is commutative, so which operand is written first cannot matter.
#[test]
fn the_written_order_of_the_operands_does_not_matter() {
    for source in [
        r#"out: {p: int | *5, c: "v\(p)"} & {p: 9}"#,
        r#"out: {p: 9} & {p: int | *5, c: "v\(p)"}"#,
    ] {
        assert_eq!(
            eval_to_json(source).unwrap(),
            json!({"out": {"p": 9, "c": "v9"}}),
            "{source}"
        );
    }
}

/// Not an interpolation bug: a plain reference is wrong the same way, which is what makes
/// this a unification defect rather than a string one.
#[test]
fn a_plain_reference_to_an_overridden_default_is_not_stale() {
    let json = eval_to_json("x: {p: int | *5, c: p}\nout: x & {p: 9}").unwrap();
    assert_eq!(json["out"], json!({"p": 9, "c": 9}));
    // The untouched definition still reads its default. This half already works, and a
    // fix must keep it working.
    assert_eq!(json["x"], json!({"p": 5, "c": 5}));
}

/// Every default kind, not just integers. `bool` and `string` matter because a stale one
/// silently flips behaviour instead of failing.
#[test]
fn every_kind_of_default_is_overridden_for_its_readers() {
    for (source, expected) in [
        (
            r#"x: {d: string | *"/a", c: "\(d)/f"}
out: x & {d: "/b"}"#,
            json!({"d": "/b", "c": "/b/f"}),
        ),
        (
            r#"x: {b: bool | *false, c: "\(b)"}
out: x & {b: true}"#,
            json!({"b": true, "c": "true"}),
        ),
        (
            r#"x: {p: (>0 & <100) | *5, c: "v\(p)"}
out: x & {p: 9}"#,
            json!({"p": 9, "c": "v9"}),
        ),
        (
            r#"x: {l: [...int] | *[1], c: len(l)}
out: x & {l: [1, 2, 3]}"#,
            json!({"l": [1, 2, 3], "c": 3}),
        ),
    ] {
        assert_eq!(eval_to_json(source).unwrap()["out"], expected, "{source}");
    }
}

/// The reader does not have to sit beside the field. A nested struct and a `let` both
/// reach the same value and must both see the override.
#[test]
fn readers_below_the_field_see_the_override_too() {
    let nested = eval_to_json(
        r#"x: {p: int | *5, n: {c: "v\(p)"}}
out: x & {p: 9}"#,
    )
    .unwrap();
    assert_eq!(nested["out"], json!({"p": 9, "n": {"c": "v9"}}));

    let bound = eval_to_json(
        r#"x: {p: int | *5, let q = p, c: "v\(q)"}
out: x & {p: 9}"#,
    )
    .unwrap();
    assert_eq!(bound["out"], json!({"p": 9, "c": "v9"}));
}

/// Through a definition, which is how the shape is actually written.
#[test]
fn a_definition_unified_with_an_override_propagates_it() {
    assert_eq!(
        eval_to_json(
            r#"#Svc: {p: int | *5, c: "v\(p)"}
out: #Svc & {p: 9}"#
        )
        .unwrap()["out"],
        json!({"p": 9, "c": "v9"})
    );
}

/// The regression this must not introduce: with nothing overriding it, every reader still
/// sees the default.
#[test]
fn an_untouched_default_still_reaches_its_readers() {
    assert_eq!(
        eval_to_json(
            r#"x: {p: int | *5, c: "v\(p)"}
out: x"#
        )
        .unwrap(),
        json!({"x": {"p": 5, "c": "v5"}, "out": {"p": 5, "c": "v5"}})
    );
}

/// The whole `pkgs/services.cue` shape in one case, so a fix can be checked against what
/// broke rather than against the reduction.
#[test]
fn a_service_preset_threads_its_overrides_into_command_and_environment() {
    let json = eval_to_json(
        r#"
#Postgres: {
    port:    (>0 & <65536) | *5432
    dataDir: string | *".enve/data/postgres"
    user:    string | *"postgres"
    command: string | *"postgres -D \(dataDir) -p \(port)"
    environment: {
        PGPORT:       "\(port)"
        PGDATA:       dataDir
        DATABASE_URL: "postgresql://\(user)@localhost:\(port)/"
    }
}
postgres: #Postgres & {
    port:    15432
    dataDir: "/dev/shm/pg_15432"
    user:    "analytics"
}
"#,
    )
    .unwrap();

    assert_eq!(
        json["postgres"]["command"],
        "postgres -D /dev/shm/pg_15432 -p 15432"
    );
    assert_eq!(
        json["postgres"]["environment"],
        json!({
            "PGPORT": "15432",
            "PGDATA": "/dev/shm/pg_15432",
            "DATABASE_URL": "postgresql://analytics@localhost:15432/",
        })
    );
}

/// Without a default the same staleness rejected a file upstream accepts: the
/// reader has to be derived at the vertex, not made concrete before it.
#[test]
fn a_reader_of_a_field_with_no_default_sees_the_override() {
    assert_eq!(
        eval_to_json(r#"out: {p: int, c: "v\(p)"} & {p: 9}"#).unwrap(),
        json!({"out": {"p": 9, "c": "v9"}})
    );
}

/// The counterpart, and the line a fix must not cross. Upstream reports `x` as an
/// incomplete `int` with an invalid interpolation; deriving its reader again at
/// another vertex must not make `x` itself concrete.
#[test]
fn an_incomplete_field_stays_incomplete_where_it_is_written() {
    assert!(
        eval_to_json(
            r#"x: {p: int, c: "v\(p)"}
out: x & {p: 9}"#
        )
        .is_err()
    );
}

/// Overriding an override conflicts, as it does upstream. Only the message
/// differs there, so this asserts the rejection alone.
#[test]
fn a_second_override_of_the_same_field_conflicts() {
    assert!(
        eval_to_json(
            r#"x: {p: int | *5, c: "v\(p)"}
out: x & {p: 9}
out2: out & {p: 11}"#
        )
        .is_err()
    );
}

/// A binding read by a nested literal that declares a field of the same name.
/// Upstream v0.16.1 gives every probe the overridden port, and the untouched
/// definition beside it keeps the default.
///
/// This is the shape that made deriving again diverge: the nested `port` is
/// rebuilt from the binding, which is rebuilt from the outer `port`, so a
/// derivation that treats every rebuilt value as a change never settles.
#[test]
fn a_binding_reaches_nested_literals_that_reuse_its_field_name() {
    let json = eval_to_json(
        r#"
#Postgres: {
    port: (>0 & <65536) | *5432
    let servicePort = port
    healthCheck: {port: (>0 & <65536) | *servicePort, command: "pg_isready -p \(port)"}
    readiness: {port: (>0 & <65536) | *servicePort}
}
postgres: #Postgres & {port: 15432}
plain: #Postgres
"#,
    )
    .unwrap();

    assert_eq!(
        json["postgres"],
        json!({
            "port": 15432,
            "healthCheck": {"port": 15432, "command": "pg_isready -p 15432"},
            "readiness": {"port": 15432},
        })
    );
    assert_eq!(
        json["plain"],
        json!({
            "port": 5432,
            "healthCheck": {"port": 5432, "command": "pg_isready -p 5432"},
            "readiness": {"port": 5432},
        })
    );
}

/// The same defect one package boundary away, which is where `enve` actually meets it:
/// `pkgs.#PostgresService` is imported, not written beside the override.
///
/// Fixed 2026-09-20 for a definition in the same source; this path was still stale
/// afterwards, so it is asserted separately rather than folded into the cases above.
/// Through the import, upstream `cue` v0.16.1 gives `command: "postgres … -p 15432"` and
/// `DATABASE_URL: postgresql://posthog@localhost:15432/postgres`, and `enve` gave 5432
/// for both.
#[test]
fn an_imported_definition_propagates_the_importers_overrides() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/defaulted_field_references/overridden.cue");
    let (evaluator, root) = PackageLoader::load_file(fixture).unwrap();
    let result = evaluator.to_json(root).unwrap();

    assert_eq!(
        result["overridden"],
        json!({
            "size": 15432,
            "root": "/dev/shm/r",
            "who": "analytics",
            "label": "run -r /dev/shm/r -n 15432",
            "copy": 15432,
            "inner": {"tag": "analytics@15432"},
        })
    );
    // Untouched across the boundary too: the defaults still reach their readers.
    assert_eq!(
        result["byDefault"],
        json!({
            "size": 5432,
            "root": "/var/lib",
            "who": "nobody",
            "label": "run -r /var/lib -n 5432",
            "copy": 5432,
            "inner": {"tag": "nobody@5432"},
        })
    );
}

// ---------------------------------------------------------------------------
// Across the import boundary.
//
// A package used to be evaluated in an arena of its own and deep-copied into the
// importer's, which could not carry a field's recipes and dropped them, so an
// imported field arrived as a value that reads nothing and never derived again.
// It is now evaluated into the importer's arena, and the file it was written in
// carries its own import set, so `pkg.Name` inside a recipe still means what that
// file bound it to. Every expectation below is `cue export` v0.16.1.
// ---------------------------------------------------------------------------

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/defaulted_field_references")
        .join(name)
}

fn export(name: &str) -> serde_json::Value {
    let (evaluator, root) = PackageLoader::load_file(fixture(name)).unwrap();
    evaluator.to_json(root).unwrap()
}

/// An imported recipe that reads a package **it** imported, derived again at the
/// importer's override. Before the import set moved onto the environment a recipe
/// captures, this derived under the importer's imports, where `naming` is absent.
#[test]
fn an_imported_recipe_resolves_the_packages_its_own_file_imported() {
    let result = export("nested_import.cue");

    assert_eq!(
        result["viaOverride"],
        json!({"size": 15432, "label": "svc-15432", "copy": 15432})
    );
    assert_eq!(
        result["viaDefault"],
        json!({"size": 5432, "label": "svc-5432", "copy": 5432})
    );
}

/// The importer and the imported file both bind `naming`, to different packages.
/// One evaluator-wide alias map cannot hold two, and answers whichever it kept:
/// `host-15432`, the importer's package, inside a recipe that never named it.
#[test]
fn two_files_may_bind_one_identifier_to_two_packages() {
    let result = export("colliding_aliases.cue");

    assert_eq!(result["hostPrefix"], json!("host"));
    assert_eq!(
        result["overridden"],
        json!({"size": 15432, "label": "svc-15432", "copy": 15432})
    );
}

/// Three packages deep - `stacks` reads `presets` reads `naming` - with the
/// override written at the top. `summary` reads a field of the nested merge, so
/// it settles only once the descent into `preset` has.
#[test]
fn an_override_reaches_readers_three_packages_away() {
    assert_eq!(
        export("depth_three.cue")["stack"],
        json!({
            "preset": {"size": 15432, "label": "svc-15432", "copy": 15432},
            "summary": "svc-15432/15432",
        })
    );
}

/// The same package imported by two files of one package, each overriding it
/// differently. The two merges must not see each other.
#[test]
fn two_files_of_a_package_override_one_import_independently() {
    let (evaluator, root) = PackageLoader::load_dir(fixture("shared")).unwrap();
    let result = evaluator.to_json(root).unwrap();

    assert_eq!(result["alpha"]["copy"], json!(1111));
    assert_eq!(result["alpha"]["label"], json!("run -r /var/lib -n 1111"));
    assert_eq!(result["beta"]["copy"], json!(2222));
    assert_eq!(result["beta"]["label"], json!("run -r /var/lib -n 2222"));
}

/// An import that resolves to nothing. The loader lends its arena to the package
/// it is loading, so a package that fails to load must hand it back: the file
/// still loads, and still reports what it reported before.
#[test]
fn an_import_that_cannot_be_loaded_leaves_the_importer_intact() {
    let (evaluator, root) = PackageLoader::load_file(fixture("missing_import.cue"))
        .expect("the file itself still loads");
    let error = evaluator
        .to_json(root)
        .expect_err("the unresolvable reference is still an error");

    assert!(
        error.contains(r#"reference "nosuch" not found"#),
        "unexpected error: {error}"
    );
}

/// A recipe that derives to a reference nothing can resolve keeps the value it
/// had - the rule that makes re-derivation safe against a forward reference.
/// A disjunction used to hide such a reference from that test, so the default
/// branch was written as bottom and the field exported as `_|_`.
///
/// `let b = a2` above `let a2 = p` is the open forward-`let` gap, so upstream's
/// `"v9"` is still out of reach here; what this pins is that the value stays a
/// string rather than becoming an export failure.
#[test]
fn an_unresolved_reference_inside_a_disjunction_keeps_the_previous_value() {
    assert_eq!(
        eval_to_json(
            r#"
x: {
	let b = a2
	let a2 = p
	p: int | *5
	c: string | *"v\(b)"
} & {p: 9}
"#
        )
        .unwrap(),
        json!({"x": {"p": 9, "c": "v5"}})
    );
}
