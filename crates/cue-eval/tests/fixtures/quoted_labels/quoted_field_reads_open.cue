// Read from a plain struct, the field "#e" is not a definition: it stays open.
s: {"#e": {a: 1}, #e: {c: 3}}
y: s."#e" & {b: 2}
