P: {
	features: [...string] | *["a"]
	let all = [for f in features {f + "!"}]
	let n = len(all)
	if n > 0 {x: all}
}
p1: P & {features: ["a", "b"]}
