#Config: {disj: _ | *{}, shared: _}
#Env: {
	conf: [string]: #Config
	conf: one: shared: "foo"
}
x: [string]: {
	conf: ["one"]: disj: {} | _
	conf: two: shared: conf.one.shared
}
x: env1: #Env
