#P: {
	a: int | *0
	l: #L
	#L: {if a > 0 {c: 2}}
}
x: #P & {a: 1, l: c: 2}
