#P: {
	l: [...string] | *["a"]
	pre: string | *"p"
	for i, v in l {"f\(i)": "\(pre)-\(v)"}
}
x: #P & {l: ["b", "c"]}
y: #P & {pre: "q"}
z: x & {pre: "r"}
