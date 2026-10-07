P: {
	a: int | *0
	b: int | *0
	if true {x: a + b}
}
p1: P & {a: 1}
p2: p1 & {b: 2}
