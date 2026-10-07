#Env: {
	conf: one: shared: "foo"
}
env1: #Env
[string]: {
	conf: ["one"]: disj: {}
	conf: two: shared: conf.one.shared
}
