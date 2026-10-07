P: {
	let L = [for k, _ in s {k}]
	x: close({n: len(L)})
	s: {}
}
p: P & {s: {a: 1}}
