import "strings"

// A let holding a comprehension over a field declared after the reader.
let L = [for f in features {f}]
b: strings.Join(L, ",")
features: ["a", "b"]
