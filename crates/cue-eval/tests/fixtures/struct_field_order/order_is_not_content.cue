// Arc order is not part of a value: two structs that list the same fields in
// another order are equal, and as disjuncts they are one.
eq: {
	flat:      {a: 1, b: 2} == {b: 2, a: 1}
	nested:    {x: {a: 1, b: 2}} == {x: {b: 2, a: 1}}
	unified:   ({b: 2} & {a: 1}) == {a: 1, b: 2}
	ne:        {a: 1, b: 2} != {b: 2, a: 1}
	different: {a: 1, b: 2} == {b: 1, a: 2}
	inList:    [{a: 1, b: 2}] == [{b: 2, a: 1}]
}
_S: {b: 2, a: 1}
dedup: {a: 1, b: 2} | _S
dedupDefault: *{a: 1, b: 2} | {b: 2, a: 1}
kDedup: [for k, _ in dedup {k}]
