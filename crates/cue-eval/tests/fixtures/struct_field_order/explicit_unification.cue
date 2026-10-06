// Explicit unification and embedded literals: the left conjunct's fields first, then the ones the right adds.
// Each k* list is the order a comprehension yields the fields in.

s1: {b: 1, a: 2} & {c: 3, a: 2}
k1: [for k, v in s1 {k}]
s2: {x: 1} & {a: 1} & {m: 1, x: 1}
k2: [for k, v in s2 {k}]
#D: {z: int, y: int}
s3: #D & {y: 1, z: 2}
k3: [for k, v in s3 {k}]
s4: {y: 1, z: 2} & #D
k4: [for k, v in s4 {k}]
s5: {
	{q: 1, p: 2}
	r: 3
	p: 2
}
k5: [for k, v in s5 {k}]
s6: {
	r: 3
	{q: 1, p: 2}
}
k6: [for k, v in s6 {k}]
s7: {c: 1, b: 2} & {a: 1, b: 2, d: 1} & {e: 1, a: 1}
k7: [for k, v in s7 {k}]
s8: {b: {y: 1}, a: 1} & {a: 1, b: {x: 2}}
k8: [for k, v in s8.b {k}]
s9: S1 & S2
S1: {b: 1, a: 1}
S2: {c: 1, a: 1}
k9: [for k, v in s9 {k}]
s10: {a: 1, b: 2} & {for k in ["c", "a"] {(k): 1}}
k10: [for k, v in s10 {k}]
#E: {n: int, m: int, ...}
s11: #E & {o: 1, m: 2, n: 3}
k11: [for k, v in s11 {k}]
