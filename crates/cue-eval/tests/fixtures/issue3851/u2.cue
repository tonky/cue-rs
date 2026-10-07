#Config: {disj: _ | *{}, shared: _}
env1: conf: [string]: #Config
env1: conf: one: shared: "foo"
[string]: {
	conf: two: shared: conf.one.shared
}
