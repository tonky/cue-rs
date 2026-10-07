#A: {kind: "a", a: int | *0, if a > 0 {b: int}}
#B: {kind: "b", b: string}
x: #A | #B
x: {kind: "a", a: 1, b: 2}
