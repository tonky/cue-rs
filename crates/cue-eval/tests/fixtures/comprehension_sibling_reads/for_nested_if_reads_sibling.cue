P: {
	l: [1, 2]
	k: int | *10
	for v in l {
		if v > 0 {"f\(v)": v + k}
	}
}
p1: P & {k: 20}
