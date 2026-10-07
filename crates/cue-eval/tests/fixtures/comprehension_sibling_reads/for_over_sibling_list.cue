P: {
	l: [...int] | *[1, 2]
	for i, v in l {"f\(i)": v}
}
p1: P & {l: [5, 6]}
