env1: conf: one: {disj: _ | *{}, shared: "foo"}
[string]: {
	conf: one: disj: {} | _
	conf: two: shared: conf.one.shared
}
