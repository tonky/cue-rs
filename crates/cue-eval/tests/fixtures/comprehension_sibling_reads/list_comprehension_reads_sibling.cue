P: {
	k: int | *1
	l: [for v in [1, 2] {v + k}]
	if true {m: [for v in [1, 2] {v * k}]}
}
p1: P & {k: 10}
