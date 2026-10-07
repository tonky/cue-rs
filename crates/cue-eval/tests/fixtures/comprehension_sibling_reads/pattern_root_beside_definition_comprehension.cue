// As pattern_root_beside_definition, with the definition's field generated
// by a comprehension (upstream issue3851 t2).
issue3851: t2: {
	#Top: _
	#Config: {
	disj: _ | *{}
	shared: _
	}
	#Env: {
	conf: [string]: #Config
	if true {
		conf: one: shared: "foo"
	}
	}
	env1: #Env
	[string]: {
	conf: ["one"]: disj: {} | #Top
	conf: two: shared: conf.one.shared
	}
}
