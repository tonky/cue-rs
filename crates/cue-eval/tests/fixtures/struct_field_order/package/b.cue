package p

jobs: zeta: {n: 1}
#Job: {steps: [...string], name: string, image: *"x" | string}
jobs: alpha: #Job & {name: "a", steps: ["s"]}
