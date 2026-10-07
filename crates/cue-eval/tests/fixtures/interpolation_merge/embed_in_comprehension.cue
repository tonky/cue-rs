#Task: {name?: string, command: string}
#C: {worker?: string, lint?: string | #Task}
_onMac: {worker: "mac", ...}
_tmpl: {_onMac, #package: string, lint: {command: "cargo clippy -p \(#package)"}, ...}
_names: ["a", "b"]
components: [string]: #C
components: {for n in _names {(n): _tmpl & {#package: n}}}
