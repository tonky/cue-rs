#D: {
	features: [...string]
	for f in features for n in L[f] {(n): true}
}
r: #D & {features: ["b"]}
L: {
	"a": ["x", "y"]
	"b": ["z"]
}
