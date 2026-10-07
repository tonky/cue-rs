#Config: {shared: _}
#Env: {
	conf: [string]: #Config
	conf: one: shared: "foo"
}
env1: #Env
[string]: {
	conf: ["one"]: disj: {}
	conf: two: shared: conf.one.shared
}
