#Env: {
	conf: [string]: {shared: _}
	conf: one: shared: "foo"
}
x: {
	env1: #Env
	[string]: {
		conf: two: shared: conf.one.shared
	}
}
