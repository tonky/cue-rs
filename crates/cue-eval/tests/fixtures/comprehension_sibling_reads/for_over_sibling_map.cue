P: {
	m: [string]: int
	m: {a: *1 | int}
	for k, v in m {"g_\(k)": v}
}
p1: P & {m: a: 3}
