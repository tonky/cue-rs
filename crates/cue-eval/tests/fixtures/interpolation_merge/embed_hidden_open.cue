#Task: {name?: string, command: string}
#C: {worker?: string, lint?: string | #Task, test?: string | #Task}
#P: components?: [string]: #C
pipeline: #P & {components: x: _tmpl & {#package: "x"}}
_tmpl: {
	_onMac
	#package: string
	lint: {name: "clippy", command: "cargo clippy -p \(#package)"}
	test: "cargo test -p \(#package)"
	...
}
_onMac: {worker: "mac", ...}
