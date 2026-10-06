#S: {m: [string]: {on: bool}, for k, v in m if v.on {(k): "on"}}
v: #S & {m: {a: on: true, b: on: false}}
