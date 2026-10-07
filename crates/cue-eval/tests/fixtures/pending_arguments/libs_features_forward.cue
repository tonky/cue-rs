import "list"

// enact's shape: a library table and a feature list, both after their readers.
x: [for f in features for n in _libs[f] {name: n, of: len(_all)}]
has: list.Contains(_all, "z")
_all: [for f in features for n in _libs[f] {n}]
features: ["a", "b"]
_libs: {a: ["x", "y"], b: ["z"]}
