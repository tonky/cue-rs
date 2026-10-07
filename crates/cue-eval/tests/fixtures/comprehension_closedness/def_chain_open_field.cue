#A: {a: int | *0, if a > 0 {b: int}}
#B: #A & {c?: int}
x: #B & {a: 1, b: 2}
