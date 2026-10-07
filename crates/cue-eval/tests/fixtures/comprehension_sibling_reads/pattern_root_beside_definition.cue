// A root pattern meets only regular fields: #Env stays as declared, and
// env1's conf.two reads the shared value the definition sets on conf.one.
#Config: {disj: _ | *{}, shared: _}
#Env: {
	conf: [string]: #Config
	conf: one: shared: "foo"
}
env1: #Env
[string]: {
	conf: ["one"]: disj: {} | _
	conf: two: shared: conf.one.shared
}
