// Queued conjuncts add their fields in queue order: references and dynamic labels, then what comprehension bodies compute, then disjunctions.
// Each k* list is the order a comprehension yields the fields in.

n: "z"
S1: {b: 1, a: 1}
S2: {e: 1, d: 1}
D: *{y: 1} | {x: 1}
v1: {(n): 1, S1}
v2: {S1, (n): 1}
v3: {for k in ["q"] {(k): 1}, (n): 1}
v4: {D, S1}
v5: {S1, D}
v6: {for k in ["q"] {(k): 1}, D}
v7: {S1, S2}
v8: {S2, S1}
v9: S2 & {S1}
v10: {for k in ["q"] {r: 1}, c: 1}
v11: {for k in ["q"] {S1}, c: 1}
v12: {for k in ["q"] {S1, r: 1}, for k in ["p"] {m: 1}, c: 1}

kv1: [for k, v in v1 {k}]
kv2: [for k, v in v2 {k}]
kv3: [for k, v in v3 {k}]
kv4: [for k, v in v4 {k}]
kv5: [for k, v in v5 {k}]
kv6: [for k, v in v6 {k}]
kv7: [for k, v in v7 {k}]
kv8: [for k, v in v8 {k}]
kv9: [for k, v in v9 {k}]
kv10: [for k, v in v10 {k}]
kv11: [for k, v in v11 {k}]
kv12: [for k, v in v12 {k}]
