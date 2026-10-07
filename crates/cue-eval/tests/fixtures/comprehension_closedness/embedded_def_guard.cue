#A: {a: int | *0}
#B: {#A, if a > 0 {b: int}}
x: #B & {a: 1, b: 3}
