// enve batch2: a let over a version the table has no entry for.
_b: {"1.0": {c: {dir: "x"}}}
#R: {version: string, f: [...string], let b = _b[version], list: [for k in f {b[k].dir}]}
out: #R & {version: "0.0.1", f: ["c"]}
