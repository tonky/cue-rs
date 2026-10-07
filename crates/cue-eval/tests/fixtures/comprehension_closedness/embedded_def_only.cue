#A: {a: int | *0, if a > 0 {b: int}}
#B: {#A}
x: #B & {a: 1, b: 3}
