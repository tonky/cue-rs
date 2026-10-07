#Config: {disj: _ | *{}, shared: _}
#Env: {
	conf: [string]: #Config
	conf: one: shared: "foo"
}
T: {
	conf: ["one"]: disj: {} | _
	conf: two: shared: conf.one.shared
}
env1: #Env & T
