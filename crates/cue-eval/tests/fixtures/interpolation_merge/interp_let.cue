_t: {_onMac, #p: string, let cmd = "cargo test -p \(#p)", test: cmd}
_onMac: {worker: "mac", ...}
x: {test?: string, ...} & _t & {#p: "a"}
