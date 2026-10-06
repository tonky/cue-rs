// Dynamic labels and comprehensions: static labels where they are declared, computed ones after.
// Each k* list is the order a comprehension yields the fields in.

n: "z"
S1: {b: 1, a: 1}
u1: {(n): 1, a: 2}
u2: {a: 2, (n): 1, b: 1}
u3: {for k in ["q"] {(k): 1}, a: 1}
u4: {S1, for k in ["q"] {(k): 1}, c: 1}
u5: {for k in ["q"] {(k): 1}, S1, c: 1}
u6: {a: b, b: 1}
u7: {c: S1.a, S1}
u8: *{y: 1, x: 1} | {x: 1}
u9: u8 & {w: 1}
u10: {w: 1} & (*{y: 1, x: 1} | {x: 1})
u12: {w: 1} & {"q-r": 1, w: 1, _h: 1, #d: 1, v: 1}
u13: S1 & S1 & {c: 1}
u14: {d: 1} & {S1, c: 1}
u15: {S1 & {c: 1}, d: 1}
u16: {[string]: int, z: 1, a: 1} & {m: 1}

ku1: [for k, v in u1 {k}]
ku2: [for k, v in u2 {k}]
ku3: [for k, v in u3 {k}]
ku4: [for k, v in u4 {k}]
ku5: [for k, v in u5 {k}]
ku6: [for k, v in u6 {k}]
ku7: [for k, v in u7 {k}]
ku8: [for k, v in u8 {k}]
ku9: [for k, v in u9 {k}]
ku10: [for k, v in u10 {k}]
ku12: [for k, v in u12 {k}]
ku13: [for k, v in u13 {k}]
ku14: [for k, v in u14 {k}]
ku15: [for k, v in u15 {k}]
ku16: [for k, v in u16 {k}]
