#Env: {
	conf: [string]: {disj: *1 | int, shared: _}
	conf: one: shared: "foo"
}
env1: #Env
[string]: {
	conf: ["one"]: disj: 1 | int
	conf: two: shared: conf.one.shared
}
