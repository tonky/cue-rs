features: ["a", "b"]
_libs: {a: ["x", "y"], b: ["z"]}
let all = [for f in features for n in _libs[f] {n}]
x: [for f in features for n in _libs[f] {name: n, of: len(all)}]
