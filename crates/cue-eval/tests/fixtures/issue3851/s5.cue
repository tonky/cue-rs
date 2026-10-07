env1: conf: one: {disj: *1 | 2, shared: "foo"}
[string]: {
	conf: one: disj: 1 | 2
	conf: two: shared: conf.one.shared
}
