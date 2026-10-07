#A: {a: int | *0}
#B: {a: int, b: {if a > 0 {c: 1}}}
#P: #A & #B
x: #P & {a: 1, b: c: 1}
