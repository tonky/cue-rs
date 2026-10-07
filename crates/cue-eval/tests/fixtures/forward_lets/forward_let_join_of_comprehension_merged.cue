import "strings"
P: {
	features: [...string] | *["a"]
	let joined = strings.Join(all, ",")
	let all = [for f in features {f + "!"}]
	x: joined
	if len(all) > 1 {many: true}
}
p1: P & {features: ["a", "b"]}
