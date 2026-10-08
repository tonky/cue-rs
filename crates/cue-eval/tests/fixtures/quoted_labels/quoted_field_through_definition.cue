// A quoted "#e" is a regular field of #D, closed like any other.
#D: {"#e": {a: int}, #e: {d: string}}
x: #D & {"#e": {a: 1}, #e: {d: "s"}}
y: x."#e"
z: x.#e
