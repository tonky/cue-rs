_t: {_onMac, p: string, args: ["-p", "\(p)"]}
_onMac: {worker: "mac", ...}
x: {args?: [...string], ...} & _t & {p: "a"}
