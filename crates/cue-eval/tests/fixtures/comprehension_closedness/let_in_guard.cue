#P: {
	a: int | *0
	n: { let z = a, if z > 0 { c: 2 } }
}
x: #P & {a: 1, n: c: 2}
