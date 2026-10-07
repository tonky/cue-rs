P: {
	l: [...int] | *[1]
	add: int | *0
	for v in l {"f\(v)": v + add}
}
p1: P & {l: [1, 2], add: 10}
