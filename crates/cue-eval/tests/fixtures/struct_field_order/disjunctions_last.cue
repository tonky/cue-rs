// A disjunction resolves after everything beside it.
// Each k* list is the order a comprehension yields the fields in.

S1: {b: 1, a: 1}
D: *{y: 1} | {x: 1}
_E: {y: 1, x: 1} | {x: 2}
w1: {*{y: 1} | {x: 1}, S1}
w2: {_E, S1}
w2: x: 1
w3: {S1, _E}
w3: x: 1
w4: D & S1
w5: S1 & D
w6: {D, c: 1}
w7: _E & {x: 1}
w8: {x: 1} & _E

kw1: [for k, v in w1 {k}]
kw2: [for k, v in w2 {k}]
kw3: [for k, v in w3 {k}]
kw4: [for k, v in w4 {k}]
kw5: [for k, v in w5 {k}]
kw6: [for k, v in w6 {k}]
kw7: [for k, v in w7 {k}]
kw8: [for k, v in w8 {k}]
