#Task: {name?: string, command: string}
#C: {worker?: string, lint?: string | #Task, test?: string | #Task, jobs?: [string]: _}
#P: {
	components?: [string]: #C
	#jobs: {
		for _, c in components for k, v in c {
			if k == "jobs" for n, _ in v {(n): n}
			if k == "lint" || k == "test" {(k): k}
		}
	}
	#jobName: or([for k, _ in #jobs {k}])
	workflows?: [string]: stages: [...{select: [...#jobName]}]
}
_onMac: {worker: "mac", ...}
_tmpl: {
	_onMac
	#package: string
	lint: {name: "clippy", command: "cargo clippy -p \(#package)"}
	test: "cargo test -p \(#package)"
	...
}
let J = pipeline.#jobs
pipeline: #P & {
	components: {
		x: _tmpl & {#package: "x"}
		y: {jobs: ready: {}}
	}
	workflows: local: stages: [{select: [J.lint, J.test, J.ready]}]
}
