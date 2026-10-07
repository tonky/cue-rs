import "strings"
_t: {m: int, s: string & strings.MinRunes(m)}
x: {s: string} & _t & {m: 2, s: "abc"}
