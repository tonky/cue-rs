P: {
	base: int | *1000
	m: [string]: {on: bool | *true, port: int}
	m: {a: port: 1, b: port: 2}
	for k, v in m {
		if v.on {"\(k)": v.port + base}
	}
}
p1: P & {base: 0}
p2: P & {m: b: on: false}
p3: P & {m: c: port: 3}
