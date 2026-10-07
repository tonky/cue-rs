#P: {
	a: int | *0
	n: { for i, v in [a] if v > 0 { "c\(i)": v } }
}
x: #P & {a: 1, n: c0: 1}
