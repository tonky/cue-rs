// Known divergence: the fields a comprehension adds under computed labels
// come after the references beside the literal upstream.

S1: {b: 1, a: 1}
x: {for k in ["c"] {(k): 1}} & S1
kx: [for k, v in x {k}]
