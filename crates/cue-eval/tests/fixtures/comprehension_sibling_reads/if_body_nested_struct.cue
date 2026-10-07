P: {
	a: string | *"d"
	if true {
		inner: {v: a, w: "w-\(a)"}
	}
}
p1: P & {a: "z"}
