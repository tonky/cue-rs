#C: {worker?: string, lint?: string, test?: string}
_flags: {mac: true}
_tmpl: {
	if _flags.mac {_onMac}
	#package: string
	lint: "cargo clippy -p \(#package)"
	...
}
_onMac: {worker: "mac", ...}
x: #C & _tmpl & {#package: "x"}
