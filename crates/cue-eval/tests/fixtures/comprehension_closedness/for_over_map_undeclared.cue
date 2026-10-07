#P: {deps: [string]: string, env: {for k, v in deps {"DEP_\(k)": v}}}
x: #P & {deps: a: "1", env: OTHER: "1"}
