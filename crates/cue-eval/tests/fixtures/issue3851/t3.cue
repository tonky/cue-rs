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
