L: {
	"a": ["x", "y"]
	"b": L["a"]
}
#D: {
	features: [...string]
	out: {for f in features for n in L[f] {(n): true}}
}
r: #D & {features: ["b"]}
