# String literal forms, and the trivia a format has to give back

Phases 01-05 were all evaluator work. This one is the layer under it: the lexer
and the parser threw away which of CUE's four string spellings a literal was
written in, and the formatter threw away every comment in the file. Both cost
`enve` directly - a block literal in a `files:` entry evaluated with the
surrounding indentation in the value, a raw literal had its backslashes
resolved, and `enve cue fmt` rewrote a module in place without its comments.

## What was wrong

Four `#[regex]` rules, one per spelling, each producing a bare `String`. Three
consequences, in order of how much they cost:

- The form was gone. `Expr::String` held a value with no record of how it had
  been written, so the formatter wrote every one as `"…"`. A block literal's
  newlines then landed inside single quotes and the file did not reparse.
- The raw forms were unescaped anyway. `#"C:\new\table"#` evaluated to `C:`, a
  newline, `ew`, a tab, `able` - the guard that is the entire point of the form
  was not read.
- A regex cannot find the end of a CUE string. `#"([^"])*"#` cannot express
  `#"say "hi""#`, whose closing delimiter is `"#`.

Comments and blank lines were dropped at the lexer with nothing downstream.

## What was built

One lexer rule per *opening* delimiter - `#*"` and `#*'` - then a hand-written
scan for the close, which knows what it is looking for because it has just read
what opened it. The token became `RawString { text, form }`, still encoded, and
decoding moved to the parser where position information for a diagnostic lives.
`StringLit { value, form }` carries both halves into the AST: the value is what
evaluation sees, the form is what the formatter needs.

Decoding follows CUE's own order - dedent a block literal, resolve escapes under
the form's guard, then split on `\(…)` under the same guard - and refuses what
CUE refuses, by name and at a position: `'\q'` at `5..7`, `\x41` named as a
bytes escape used in a string, a single-line literal that runs past its line
named as unterminated.

Comments are read out of the *gaps between tokens* rather than made tokens of
their own, so no production has to skip one while peeking, and a `//` inside a
string belongs to that token and never appears in a gap. They land as
`Decl::Comment` and `Decl::BlankLine` in source order, plus `SourceFile::header`
for what stands above `package`.

## Trade-offs

**Comment fidelity is a table, not a promise.** Only a comment standing where a
declaration could stand survives; one inside a list literal, between an
expression's operands, or trailing a declaration on the same line is dropped.
Attaching trivia to every node instead would have meant a field on `FieldDecl`,
`Label`, `StructLit` and every visitor and folder. Two `Decl` variants cost
almost nothing and cover the positions a config file actually comments. The
limit is written down as an enumerated table in `comment_positions.rs`, so a
position changing sides is a diff to the table first.

**A trailing comment is dropped rather than moved.** It could have been hoisted
above the next declaration, where it would read as documenting something it does
not. Losing it is the smaller lie.

**The formatter falls back to quoting.** A raw literal cannot hold its own
closing delimiter, a block literal cannot hold a bare `"""`. No source produces
these; a `Folder` rewriting values does. Emitting a file that reparses to the
same value matters more than keeping the spelling, so the form is checked
against the value and dropped when it cannot hold it.

**Indentation is a tab.** `cue fmt` emits tabs, so spaces would have rewritten
every indented line of a module the first time `enve cue fmt` ran over it.

**A bytes literal refuses to interpolate.** `Expr::Interpolation` carries no
delimiter, so cue-rs cannot represent an interpolation that yields bytes.
Upstream accepts `'a\(1)b'`. This phase makes it a named refusal instead of the
silent characters `a\(1)b` that HEAD produced. It is the one corpus case that
moved - see below - and it is in `follow_up.md`.

## Validation

105 workspace tests green, up from 91. Clippy clean over all targets.

The corpus reads 526/547 against HEAD's 527. The single case that moved is
`upstream_cue_testdata_interpolation_scalars.txtar`, on `bytes2b: '\(b2)'`.
Worth being precise about what that number means: the txtar harness passes a
case when it parses, evaluates and exports - it never compares against the
`@test(eq, …)` attributes in the file - so HEAD's pass there was vacuous. HEAD
also stored that file's `'a\xED\x95a'` as its own source text with the escapes
undecoded, which this phase fixes. A loud refusal replaced a quiet wrong answer,
and the counter reads it as a regression.

`fmt-check` still fails on the three files named under "rustfmt drift" in
`follow_up.md`. That drift predates this phase and is untouched here, for the
same reason phases 03-05 left it alone.

## Process note

This phase was implemented before it was written down - the reverse of 01-05.
The phase doc and this worklog were written from the diff at commit time, which
is why neither has a stage breakdown: there were no stages, only a finished
change. `06-string-literal-forms.md` is accurate about what the code does and
should not be read as a plan that was followed.
