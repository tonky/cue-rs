#A: {kind: "a", v: int | *1, if true {w: v}}
#B: {kind: "b"}
x: (#A | #B) & {kind: "a", v: 5}
