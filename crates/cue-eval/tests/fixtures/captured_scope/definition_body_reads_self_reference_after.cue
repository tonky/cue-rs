r: #D & {features: ["b"]}
#D: {
	features: [...string]
	out: {for f in features for n in L[f] {(n): true}}
}
L: {
	"a": ["x", "y"]
	"b": L.a
}
