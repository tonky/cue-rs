// A literal's fields come before those of the references, selectors and definitions beside it.
// Each k* list is the order a comprehension yields the fields in.

S1: {b: 1, a: 1}
#D: {z: int, y: int, ...}
#F: {w: int, v: int, ...}
t1: S1 & {c: 1, a: 1}
t2: {c: 1, a: 1} & S1
t3: #D & #F & {z: 1, y: 1, w: 1, v: 1}
t4: #F & #D & {z: 1, y: 1, w: 1, v: 1}
t5: #D & {b: 1, a: 1, z: 1, y: 1}
t6: {b: 1, a: 1, z: 1, y: 1} & #D
t7: {#D, y: 1, z: 2}
t8: #D
t8: {y: 1, z: 2}
t9: {y: 1, z: 2}
t9: #D
t10: #D & {y: 1, z: 1}
t11: {y: 1, z: 1} & #D
t12: S1 & {c: 1}
t13: {c: 1} & S1
t14: {S1, c: 1}
t15: {c: 1, S1}
t16: (S1 & {}) & {c: 1}
t17: {S1} & {c: 1}
t18: S1.x & {c: 1}
S1: x: {q: 1, p: 1}
t19: #D.n & {c: 1}
#D: n: {q: 1, p: 1, ...}

kt1: [for k, v in t1 {k}]
kt2: [for k, v in t2 {k}]
kt3: [for k, v in t3 {k}]
kt4: [for k, v in t4 {k}]
kt5: [for k, v in t5 {k}]
kt6: [for k, v in t6 {k}]
kt7: [for k, v in t7 {k}]
kt8: [for k, v in t8 {k}]
kt9: [for k, v in t9 {k}]
kt10: [for k, v in t10 {k}]
kt11: [for k, v in t11 {k}]
kt12: [for k, v in t12 {k}]
kt13: [for k, v in t13 {k}]
kt14: [for k, v in t14 {k}]
kt15: [for k, v in t15 {k}]
kt16: [for k, v in t16 {k}]
kt17: [for k, v in t17 {k}]
kt18: [for k, v in t18 {k}]
kt19: [for k, v in t19 {k}]
