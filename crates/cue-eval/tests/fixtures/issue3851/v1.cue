#Config: {disj: _ | *{}, shared: _}
Env: {
	conf: [string]: #Config
	conf: one: shared: "foo"
}
env1: Env
[string]: {
	conf: ["one"]: disj: {} | _
	conf: two: shared: conf.one.shared
}
