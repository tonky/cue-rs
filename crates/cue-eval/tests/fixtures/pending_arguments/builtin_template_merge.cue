import "strings"

P: {
	features: [...string] | *["a"]
	let all = [for f in features {f + "!"}]
	joined: strings.Join(all, ",")
	n: len(all)
}
p: P & {features: ["a", "b"]}
