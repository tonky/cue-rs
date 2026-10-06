// n8n: `if list != []` was false for a non-empty list, silently dropping the job.
_t: ["a/**/*.test.ts"]
_e: []
components: {
	ne: {if _t != [] {test: "ne"}}
	ln: {if len(_t) > 0 {test: "ln"}}
	eq: {if _e == [] {test: "eq"}}
	se: {if {a: 1} == {a: 1} {test: "se"}}
}
