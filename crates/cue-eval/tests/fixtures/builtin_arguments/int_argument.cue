import "strconv"
_t: {p: string, n: strconv.Atoi(p), b: strconv.FormatInt(n, 2)}
x: {b: string} & _t & {p: "5"}
