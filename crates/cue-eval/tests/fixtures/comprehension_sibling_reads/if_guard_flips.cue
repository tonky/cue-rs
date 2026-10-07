P: {
	a: int | *0
	if a > 0 {x: a}
	if a == 0 {y: a}
}
p1: P & {a: 2}
