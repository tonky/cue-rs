import "strings"
_t: {s: string, n: len(s), u: strings.ToUpper(s)}
x: {n: int} & _t & {s: "abc"}
