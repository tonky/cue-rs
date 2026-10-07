#Env: {
	conf: [string]: {disj: *{} | _, shared: _}
	conf: one: shared: "foo"
}
env1: #Env
[string]: {
	conf: two: shared: conf.one.shared
}
