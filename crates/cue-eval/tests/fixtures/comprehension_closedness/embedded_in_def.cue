#P: {
	a: int | *0
	n: { if a > 0 { c: 2 } }
}
#W: {#P, extra: string | *"e"}
x: #W & {a: 1, n: c: 2}
