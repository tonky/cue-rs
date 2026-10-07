env1: conf: one: {disj: _ | *{}, shared: "foo"}
[string]: {
	conf: two: shared: conf.one.shared
	conf: one: disj: {} | _
}
