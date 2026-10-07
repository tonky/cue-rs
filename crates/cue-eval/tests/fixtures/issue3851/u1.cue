#Config: {disj: _ | *{}, shared: _}
env1: conf: [string]: #Config
env1: conf: one: shared: "foo"
[string]: {
	conf: ["one"]: disj: {} | _
	conf: two: shared: conf.one.shared
}
