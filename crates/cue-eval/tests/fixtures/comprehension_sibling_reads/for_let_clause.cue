P: {
	k: int | *2
	l: [1, 2] | *[1]
	for v in l
	let s = v * k {"f\(v)": s}
}
p1: P & {k: 3}
p2: P & {l: [1, 2]}
