env1: conf: one: shared: "foo"
[string]: {
	conf: ["one"]: disj: {}
	conf: two: shared: conf.one.shared
}
