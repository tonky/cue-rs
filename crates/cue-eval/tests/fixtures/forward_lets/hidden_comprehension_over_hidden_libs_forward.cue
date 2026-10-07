x: [for f in features for n in _libs[f] {name: n, of: len(_all)}]
_all: [for f in features for n in _libs[f] {n}]
features: ["a", "b"]
_libs: {a: ["x", "y"], b: ["z"]}
