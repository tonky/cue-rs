P: {
	names: ["x", "y"]
	suffix: string | *"s"
	for n in names {"\(n)": "\(n)-\(suffix)"}
}
p1: P & {suffix: "t"}
