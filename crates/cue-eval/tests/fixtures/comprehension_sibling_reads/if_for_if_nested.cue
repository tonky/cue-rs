P: {
	k: int | *1
	on: bool | *true
	m: {a: 1, b: 2}
	if on {
		for n, v in m {
			if v > 0 {"\(n)": v * k}
		}
	}
}
p1: P & {k: 10}
p2: P & {m: {c: 3}}
