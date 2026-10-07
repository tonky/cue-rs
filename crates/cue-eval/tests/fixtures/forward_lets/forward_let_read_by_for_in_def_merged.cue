#P: {
	p: int | *0
	let b = a2
	let a2 = p
	for i in [1, 2] {"x\(i)": b + i}
}
p1: #P & {p: 3}
