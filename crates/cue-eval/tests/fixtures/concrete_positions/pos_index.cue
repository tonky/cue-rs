l: ["a", "b", "c"]
i: *1 | int
o: l[i]
s: *{a: 1} | {a: 2}
p: s["a"]
k: *"a" | string
q: {a: 5}[k]
