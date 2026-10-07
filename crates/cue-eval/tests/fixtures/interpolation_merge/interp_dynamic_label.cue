_t: {_onMac, k: string, "\(k)-job": {cmd: "run \(k)"}}
_onMac: {worker: "mac", ...}
x: {worker?: string, ...} & _t & {k: "a"}
