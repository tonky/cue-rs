# String literal forms, and the trivia a format has to give back

CUE spells a string four ways and cue-rs stored all four the same: the source
slice between the delimiters. The form was thrown away at the lexer, so nothing
downstream could tell a `"""` block from a `"…"`, or a raw `#"…"#` from an
escaped one.

```cue
services: postgres: files: "postgresql.conf": """
	listen_addresses = '*'
	port = 5432
	"""

root: #"C:\new\table"#
```

HEAD evaluates the first to the source text *with its surrounding indentation
baked in*, and resolves the second's escapes although a raw literal has none:
`C:`, a newline, `ew`, a tab, `able`. Formatting the block literal then emits a
file that does not reparse - the value's newlines are written inside `"…"`, so
the string is unterminated on the first of them.

`enve` meets both: a service's config file is a block literal in a `files:`
entry, and a policy's pattern is a raw literal.

## Why the lexer could not do it

The four spellings were four `#[regex]` rules over the whole literal. A regex
can express `#"([^"])*"#` - a raw string with no quote in it - and cannot
express the one CUE actually has, where `#"say "hi""#` is a perfectly good raw
string whose closing delimiter is `"#`, not `"`. The rules also produced three
different token variants that all carried a bare `String`, so even where the
scan was right the spelling was gone by the time the parser saw it.

## Design

One lexer rule per delimiter - `#*"` and `#*'` - matching only the *opening*
delimiter, then a hand-written scan for the close. The opening delimiter is
what distinguishes the forms: the count of `#` is the guard, and two more
quotes after it make it a block literal. The scan knows what it is looking for
because it has just read what opened it.

The token carries `RawString { text, form }`: the bytes between the delimiters,
still encoded, and a `StringForm` naming the spelling. Decoding moves to the
parser, which is where the position information for a diagnostic lives, and the
form travels into the AST beside the decoded value:

```rust
pub struct StringLit {
    pub value: String,     // what it means
    pub form: StringForm,  // how it was written
}
```

Both halves are needed. The value alone cannot be re-emitted - a newline may
have been a `\n` or a block line, a backslash may have been raw or escaped - and
the form alone is not what evaluation wants to see.

## Decoding, in CUE's order

Three steps, applied as upstream applies them:

1. A block literal loses its layout: the newline after the opening delimiter,
   the indentation of the closing delimiter stripped from every line, and the
   newline before the closing line.
2. Escapes resolve under the form's guard. With `n` hashes an escape is spelled
   `\#…#n`, so a bare `\n` in `#"C:\new"#` stays two characters. This is what
   the form is *for*.
3. `\(…)` splits the result into interpolation parts, with the same guard.

A single-line literal that runs past the end of its line is refused as
unterminated, which is how upstream reads a missing closing delimiter, and every
escape CUE does not have is refused *by name* and pointed at - `'\q'` at
`5..7`, not "somewhere in this file". A block literal is the exception: its
indentation is gone by the time the escape is read, so the whole literal is
pointed at.

## Formatting: the form, and the fallback

The formatter writes a literal back in the spelling it was read in, escaping
under that form's guard and laying a block literal out the way `cue fmt` does -
body one level in from its field, closing delimiter on its own line.

A form that cannot hold its value falls back to quoting. A raw literal cannot
hold its own closing delimiter or its own escape prefix; a block literal cannot
hold a bare `"""`. No source file produces these, but a `Folder` rewriting
values can, and the formatter's contract is to emit a file that reparses to the
same value, not to insist on a spelling.

Indentation became a tab, because that is what `cue fmt` emits. Writing spaces
rewrote every indented line of a module the first time `enve cue fmt` ran.

## Comments and blank lines

A format that drops comments rewrites someone's source into something they did
not write, and `enve cue fmt` writes in place. The lexer discards both, so the
parser reads them back out of the *gaps between tokens*: the source between
`tokens[i-1]` and `tokens[i]`, with `tokens.len()` holding the tail. Nothing in
the grammar has to skip a comment while peeking, and a `//` inside a string
belongs to that token and never appears in a gap.

They arrive as two new `Decl` variants - `Comment(String)` and `BlankLine` - in
source order, plus a `SourceFile::header` for what stands above `package`, which
cannot live in `decls` because the formatter writes those after the imports.

The cost is the limit, and it is a stated one: only a comment standing where a
declaration could stand survives. One inside a list literal, between an
expression's operands, or trailing a declaration on the same line is dropped -
the last deliberately, because `Decl` has nowhere to put it and moving it above
the next declaration would document the wrong thing.
`crates/cue-syntax/tests/comment_positions.rs` names every position and its
fate, so a position changing sides is a diff to that table first.

## Tests

- `crates/cue-eval/tests/string_literal_forms.rs`: 23 spellings and the value
  each denotes, 9 refusals and the escape each names, all confirmed against
  upstream `cue` v0.16.1 on 2026-09-22; formatting preserves the value and is
  idempotent for every one; the `postgresql.conf` regression end to end; and the
  fallback, driven through a synthetic AST because no source produces it.
- `crates/cue-syntax/tests/comment_positions.rs`: the position table above, that
  a dropped comment costs only the comment, that a kept one survives a second
  format, blank-line collapsing, and that a `//` inside a string is not one.

## Validation

105 workspace tests green, up from 91. Clippy clean.

The corpus reads 526/547, against 527 on HEAD. The one case that moved is
`upstream_cue_testdata_interpolation_scalars.txtar`, and the move is a trade
taken deliberately: it contains `bytes2b: '\(b2)'`, and `Expr::Interpolation`
carries no delimiter, so cue-rs cannot represent an interpolation that yields
bytes. It now refuses by name where HEAD silently produced the seven characters
`\(b2)`. The txtar harness only requires that a case parse, evaluate and
export - it never compares against the `@test(eq, …)` attributes - so HEAD's
pass on that file was vacuous: it also stored `'a\xED\x95a'` as its own source
text, escapes undecoded, which this phase fixes.

## Not in this phase

- Interpolation in a bytes literal. Wants a delimiter on
  `Expr::Interpolation` and bytes-valued concatenation in the evaluator; see
  `follow_up.md`.
- Comments anywhere but a declaration position, per the table above.
