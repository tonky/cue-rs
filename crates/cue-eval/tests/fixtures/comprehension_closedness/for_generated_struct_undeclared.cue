#P: {xs: [...string], for x in xs {"\(x)": {enabled: bool | *true}}}
c: #P & {xs: ["a", "b"], a: enabled: false, a: other: 1}
