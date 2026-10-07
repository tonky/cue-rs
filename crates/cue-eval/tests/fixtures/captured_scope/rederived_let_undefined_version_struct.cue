// enve batch2: the same through a struct comprehension.
_b: {"1.0": {c: {dir: "x"}}}
#R: {version: string, f: [...string], let b = _b[version], m: {for k in f {(k): b[k].dir}}}
out: #R & {version: "0.0.1", f: ["c"]}
