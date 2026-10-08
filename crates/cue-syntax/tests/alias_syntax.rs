//! Which alias spelling a file may use is upstream's: `aliasv2` (postfix
//! `~X`, `~(K,V)`) is on from language v0.18.0 or with
//! `@experiment(aliasv2)`, and refuses the prefix `X=` forms; before it, the
//! postfix forms are refused. A file whose language version nobody knows
//! accepts both.

use cue_syntax::ParseOptions;

const PREFIX: &[&str] = &[
    "X=a: 1\nb: X\n",
    "[N=string]: {name: N}\n",
    "Q=\"a-b\": 1\n",
    "X=(\"dyn\"): 1\n",
    "a: X={b: 1, c: X.b}\n",
];

const POSTFIX: &[&str] = &[
    "a~X: 1\nb: X\n",
    "a~(K,V): 1\nb: K\n",
    "[string]~(N,_): {name: N}\n",
    "\"a-b\"~(K,V): 1\n",
    "(\"dyn\")~(_,V): 1\n",
];

const OLD_STYLE: &str = "old-style alias syntax (=) is not allowed with @experiment(aliasv2); use postfix syntax (~X or ~(K,V))";
const NEEDS_EXPERIMENT: &str = "postfix alias syntax requires @experiment(aliasv2)";

fn parse(source: &str, version: Option<&str>) -> Result<(), String> {
    let options = version.map(ParseOptions::at_version).unwrap_or_default();
    cue_syntax::parse_file_with(source, &options)
        .map(drop)
        .map_err(|err| err.to_string())
}

#[test]
fn an_unknown_language_version_accepts_every_spelling() {
    for source in PREFIX.iter().chain(POSTFIX) {
        assert_eq!(parse(source, None), Ok(()), "{source}");
    }
}

#[test]
fn from_v0_18_aliases_are_postfix() {
    for version in ["v0.18.0", "v0.18.1", "v0.19.0", "v1.0.0"] {
        for source in POSTFIX {
            assert_eq!(parse(source, Some(version)), Ok(()), "{version}: {source}");
        }
        // A declaration alias `X = 1` too, which upstream no longer parses
        // before v0.18.0 either; here it stays the let it always was.
        for source in PREFIX.iter().chain(&["X = 1\na: X\n"]) {
            let err = parse(source, Some(version)).unwrap_err();
            assert!(err.contains(OLD_STYLE), "{version}: {source}: {err}");
        }
    }
}

#[test]
fn before_v0_18_aliases_are_prefix() {
    // A pre-release sorts before its release.
    for version in ["v0.17.0", "v0.9.2", "v0.18.0-alpha.1"] {
        for source in PREFIX {
            assert_eq!(parse(source, Some(version)), Ok(()), "{version}: {source}");
        }
        for source in POSTFIX {
            let err = parse(source, Some(version)).unwrap_err();
            assert!(err.contains(NEEDS_EXPERIMENT), "{version}: {source}: {err}");
        }
    }
}

#[test]
fn the_experiment_turns_postfix_on_at_any_version() {
    for version in [None, Some("v0.17.0")] {
        for source in POSTFIX {
            let source = format!("@experiment(aliasv2)\n\n{source}");
            assert_eq!(parse(&source, version), Ok(()), "{version:?}: {source}");
        }
        let err = parse("@experiment(aliasv2)\n\nX=a: 1\n", version).unwrap_err();
        assert!(err.contains(OLD_STYLE), "{version:?}: {err}");
    }
}

#[test]
fn a_let_is_no_alias() {
    for version in [None, Some("v0.17.0"), Some("v0.18.0")] {
        assert_eq!(parse("let X = 1\na: X\n", version), Ok(()), "{version:?}");
    }
}
