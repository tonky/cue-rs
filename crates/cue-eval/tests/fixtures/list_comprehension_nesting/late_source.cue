// The comprehension source resolves after the list is first seen.
a: [[for k in src {k}], [0, for k in src {k + 1}]]
src: [1, 2]
b: [for v in a {[for w in v {w * 10}]}]
