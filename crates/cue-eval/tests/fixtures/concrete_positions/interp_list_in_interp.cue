import "strings"
xs: ["a", "b"]
a: "\(strings.Join([for t in xs {"(\(t))"}], ")("))"
b: "\(len([for t in xs if t != ")" {t}]))"
