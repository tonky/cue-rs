#D: {
	a: int | *0
	if true {x: a}
	for v in [1] {"y\(v)": a + v}
}
y: {#D, a: 2}
z: #D & {a: 5}
