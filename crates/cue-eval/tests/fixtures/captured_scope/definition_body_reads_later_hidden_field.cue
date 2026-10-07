#D: {
	features: [...string]
	out: [for f in features for n in _L[f] {n}]
}
r: #D & {features: ["a", "b"]}
_L: {
	"a": ["x", "y"]
	"b": ["z"]
}
