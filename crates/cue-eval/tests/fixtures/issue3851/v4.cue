#Env: {
	conf: [string]: {disj: *1 | int, shared: _}
	conf: one: shared: "foo"
}
env1: #Env
[string]: {
	conf: two: shared: conf.one.shared
}
