import "encoding/json"
_t: {v: {a: string}, s: json.Marshal(v)}
x: {s: string} & _t & {v: a: "x"}
