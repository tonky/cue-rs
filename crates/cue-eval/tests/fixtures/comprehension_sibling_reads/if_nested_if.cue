P: {
	a: int | *0
	b: bool | *true
	if true {
		if b {x: a + 1}
	}
}
p1: P & {a: 4}
