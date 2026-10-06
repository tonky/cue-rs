import "list"
l: *[1, 2] | [...int]
a: list.Contains(l, 2)
b: list.Contains([[1]], [1])
c: list.Contains([{a: 1}], {a: 1})
