_B: {a: 1}
D: {
	features: [...string]
	fetch: [for browser in features {_B[browser]}]
}
r: D & {features: ["a", "z"]}
