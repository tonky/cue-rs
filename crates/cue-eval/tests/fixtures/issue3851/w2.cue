#Env: {
	conf: [string]: {disj: _ | *{}, shared: _}
	conf: one: shared: "foo"
}
env1: #Env
[string]: {
	conf: one: disj: {} | _
	conf: two: shared: conf.one.shared
}
