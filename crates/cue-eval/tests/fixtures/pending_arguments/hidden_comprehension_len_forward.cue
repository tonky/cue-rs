// A hidden field holding a list comprehension, read by a builtin above it.
y: _all
z: len(_all)
_all: [for f in features {name: f}]
features: ["a", "b"]
