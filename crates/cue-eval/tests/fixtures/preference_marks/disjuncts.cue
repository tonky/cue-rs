b: *1 | 2 & int
twice: **1 | 2
nested: *(*1 | 2) | 3
grouped: *(int & 1) | 2
sum: *1 | 2 + 0
inner: *1 | (*2 | 3)
field: {f: *1 | 2}
read: field.f
over: {f: *"x" | string} & {f: "y"}
#D: {mode: *"fast" | "slow" & string}
d: #D
dslow: #D & {mode: "slow"}
