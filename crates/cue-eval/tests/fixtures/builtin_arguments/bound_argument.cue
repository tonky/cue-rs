import "strings"
_t: {n: int & >0, r: strings.Repeat("ab", n)}
x: {r: string} & _t & {n: 2}
