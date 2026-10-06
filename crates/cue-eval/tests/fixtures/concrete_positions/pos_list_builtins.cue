import "list"
l: *[1, 2] | [...int]
c: list.Concat([l, [3]])
r: list.Repeat(l, 2)
