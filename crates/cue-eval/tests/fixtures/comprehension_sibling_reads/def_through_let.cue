#P: {
	a: int | *0
	if true {x: a}
}
let base = #P
out: base & {a: 1}
