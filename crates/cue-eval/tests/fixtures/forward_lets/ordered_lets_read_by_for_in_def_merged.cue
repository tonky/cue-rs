#P: {
	p: int | *0
	let a2 = p
	let b = a2
	for i in [1, 2] {"x\(i)": b + i}
}
p1: #P & {p: 3}
