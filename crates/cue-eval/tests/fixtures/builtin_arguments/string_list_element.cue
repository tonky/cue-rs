import "strings"
_t: {a: string, j: strings.Join([a, "b"], ",")}
x: {j: string} & _t & {a: "x"}
