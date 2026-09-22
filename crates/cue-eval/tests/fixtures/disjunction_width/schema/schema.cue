package schema

// The shape the abort was found in: a service is one of several kinds, each
// kind a struct, and several of its fields are themselves small disjunctions.
// Nothing here is unusual - it is what a schema looks like when it describes
// alternatives. The last branch accepts any `kind`, so naming a kind does not
// narrow the value to one branch - `cue export` v0.16.1 reports this same value
// as `incomplete value {…} | {…}`. That is what keeps the disjunction alive
// across every file's unification, which is what used to double it.
#Service: {
	name:      string
	kind:      "postgres"
	protocol:  *"sql" | "tcp"
	external?: *false | bool
	host?:     *"127.0.0.1" | string
	placement?: *"shared" | "isolated" | string
} | {
	name:      string
	kind:      "redis"
	protocol:  *"redis" | "tcp"
	external?: *false | bool
	host?:     *"127.0.0.1" | string
	placement?: *"shared" | "isolated" | string
} | {
	name:      string
	kind:      *"custom" | string
	protocol?: string
	external?: *false | bool
	host?:     *"127.0.0.1" | string
	placement?: *"shared" | "isolated" | string
}

#Pipeline: {
	name?:     string
	services?: [string]: #Service
	tasks?: [string]:    string
}
