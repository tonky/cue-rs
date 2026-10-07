#S: {name: string, ...}
#P: #S & {a: int | *0, n: {if a > 0 {c: 2}}}
x: #P & {a: 1, n: c: 2}
