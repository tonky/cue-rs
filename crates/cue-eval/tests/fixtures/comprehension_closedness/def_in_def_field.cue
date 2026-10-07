#P: {a: int | *0, n: {if a > 0 {c: 2}}}
#O: {inner: #P}
x: #O & {inner: {a: 1, n: c: 2}}
