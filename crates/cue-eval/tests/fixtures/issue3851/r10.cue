env1: conf: one: shared: "foo"
[string]: {
	conf: ["one"]: {}
	conf: two: shared: conf.one.shared
}
