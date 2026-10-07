#P: {
	a: int | *0
	n: { if a > 0 { c: 2 } }
}
y: {n: c: 2}
x: #P & y & {a: 1}
