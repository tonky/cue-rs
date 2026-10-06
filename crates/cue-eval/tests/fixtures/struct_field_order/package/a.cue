package p

jobs: mid: {n: 2}
jobs: zeta: {m: 1}
order: [for k, v in jobs {k}]
fields: [for k, v in jobs.alpha {k}]
zf: [for k, v in jobs.zeta {k}]
