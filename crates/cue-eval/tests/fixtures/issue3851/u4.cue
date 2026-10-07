#Config: {disj: _ | *{}, shared: _}
env1: conf: [string]: #Config
env1: conf: one: shared: "foo"
env1: conf: one: disj: {} | _
env1: conf: two: shared: env1.conf.one.shared
