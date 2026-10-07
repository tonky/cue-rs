_b: {"1.0": {c: {dir: "x"}}}
#R: {version: string, f: [...string], let b = _b[version], list: [for k in f {b[k].dir}]}
out: #R & {version: "1.0", f: ["c"]}
