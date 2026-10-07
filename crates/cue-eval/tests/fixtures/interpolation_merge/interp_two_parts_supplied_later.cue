_t: {_onMac, a: string, n: int, l: "\(a):\(n)"}
_onMac: {worker: "mac", ...}
x: {l?: string, ...} & _t & {a: "host"} & {n: 80}
