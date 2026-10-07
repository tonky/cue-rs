L: {
	"a": ["x", "y"]
	"b": L["a"]
}
#D: {
	f: string
	out: [for n in L[f] {n}]
}
r: #D & {f: "b"}
