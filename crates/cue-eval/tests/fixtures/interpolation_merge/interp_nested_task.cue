#Task: {name?: string, command: string}
#C: {lint?: string | #Task}
_t: {#package: string, lint: {name: "clippy", command: "cargo clippy -p \(#package)"}}
x: #C & _t & {#package: "x"}
