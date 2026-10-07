#S: {
	name: string | *"n"
	if true {
		labels: app: name
	}
}
s: #S & {name: "m"}
list: [#S & {name: "q"}]
