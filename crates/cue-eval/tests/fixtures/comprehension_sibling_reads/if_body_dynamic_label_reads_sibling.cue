P: {
	name: string | *"d"
	v:    int | *0
	if true {"\(name)": v}
}
p1: P & {name: "z"}
p2: P & {v: 3}
p3: P & {name: "q", v: 4}
