#Env: {
	conf: [string]: {shared: _}
	conf: one: shared: "foo"
}
env1: #Env
[string]: {
	conf: two: shared: conf.one.shared
}
