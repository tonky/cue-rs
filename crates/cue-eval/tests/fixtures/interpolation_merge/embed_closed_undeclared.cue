#C: {worker?: string, lint?: string}
_onMac: {worker: "mac", extra: 1}
_tmpl: {_onMac, #package: string, lint: "cargo clippy -p \(#package)"}
x: #C & _tmpl & {#package: "x"}
