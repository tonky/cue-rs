_t: {_onMac, p: string, if "\(p)" == "a" {z: "is \(p)"}}
_onMac: {worker: "mac", ...}
x: {z?: string, ...} & _t & {p: "a"}
