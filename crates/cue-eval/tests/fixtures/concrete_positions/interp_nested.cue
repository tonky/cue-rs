import "strings"
xs: ["a", "b"]
out: "run \(strings.Join([for t in xs {"'!\(t)'"}], " ")) -- end"
