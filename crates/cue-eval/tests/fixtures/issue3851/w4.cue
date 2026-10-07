#Env: {
	conf: [string]: {disj: int, shared: _}
	conf: one: shared: "foo"
}
env1: #Env
[string]: {
	conf: ["one"]: disj: 1
	conf: two: shared: conf.one.shared
}
