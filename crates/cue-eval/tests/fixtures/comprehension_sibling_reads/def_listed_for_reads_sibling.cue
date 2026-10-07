#S: {
	n: int | *0
	for i in [1, 2] {"f\(i)": n + i}
}
l: [...#S]
l: [{}, {n: 10}]
m: l[1] & {n: 10}
