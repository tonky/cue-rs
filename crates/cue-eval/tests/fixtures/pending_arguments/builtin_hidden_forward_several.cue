import ("list", "strings")

a: list.Contains(_l, "x")
b: strings.Join(_l, ",")
c: list.Sort(_l, list.Ascending)
_l: [for f in ["y", "x"] {f}]
