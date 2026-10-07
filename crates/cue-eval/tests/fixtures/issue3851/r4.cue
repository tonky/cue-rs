#Config: {disj: _ | *{}, shared: _}
#Env: {
	conf: [string]: #Config
	if true {conf: one: shared: "foo"}
}
env1: #Env
[string]: {
	conf: two: shared: conf.one.shared
}
