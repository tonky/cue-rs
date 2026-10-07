import "strings"
#C: {worker?: string, u?: string}
#P: c?: [string]: #C
p: #P & {c: x: _t & {#n: "a"}}
_t: {_onMac, #n: string, u: strings.ToUpper(#n), ...}
_onMac: {worker: "mac", ...}
