issue3851: t1: {
	#Top: _
	#Config: {
	disj: _ | *"default"
	shared: _
	}
	#Schema: {
	[string]: #Config
	if true {
		one: shared: "foo"
	}
	}
	out: #Schema
	out: {
	["one"]: disj: _ | #Top
	two: shared: out.one.shared
	}
}
