#P: {
	a: int | *0
	n: { if a > 0 { c?: int } }
}
x: #P & {a: 1, n: c: 2}
