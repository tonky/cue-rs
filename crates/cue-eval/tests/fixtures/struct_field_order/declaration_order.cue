// Fields resolved in later passes, repeated declarations, optional fields, lets and patterns keep declaration order.
// Each k* list is the order a comprehension yields the fields in.

import "encoding/json"
import "encoding/yaml"

S1: {b: 1, a: 1}
x1: {a: x1.b + 1, b: 1, c: x1.a}
x2: z: 1
x2: y: 1
x2: {x: 1}
x3: {if true {q: 1}, p: 1}
x4: {a?: int, b: 1} & {c: 1, a: 1}
x5: json.Unmarshal("{\"b\":1,\"a\":2}")
x6: yaml.Unmarshal("b: 1\na: 2\n")
x7: {for k, v in S1 {(k): v}}
x8: {for k, v in S1 {"\(k)x": v}}
#C: {n: int, m: int}
x9: #C & {m: 1, n: 2}
x10: [string]: {v: 1}
x10: {q: {}, p: {}}
x11: {let t = 1, b: t, a: t}
x12: {S1 & {c: 1}}
x13: S1 & {for k in ["c"] {(k): 1}}
x15: {"b-": 1, a: 1, "0": 1}
x16: {*S1 | {z: 1}} & {c: 1}

kx1: [for k, v in x1 {k}]
kx2: [for k, v in x2 {k}]
kx3: [for k, v in x3 {k}]
kx4: [for k, v in x4 {k}]
kx5: [for k, v in x5 {k}]
kx6: [for k, v in x6 {k}]
kx7: [for k, v in x7 {k}]
kx8: [for k, v in x8 {k}]
kx9: [for k, v in x9 {k}]
kx10: [for k, v in x10 {k}]
kx11: [for k, v in x11 {k}]
kx12: [for k, v in x12 {k}]
kx13: [for k, v in x13 {k}]
kx15: [for k, v in x15 {k}]
kx16: [for k, v in x16 {k}]
