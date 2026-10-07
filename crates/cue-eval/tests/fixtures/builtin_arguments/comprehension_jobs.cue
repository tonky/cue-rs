import "strings"
_tmpl: {#package: string, lint: "lint \(strings.ToUpper(#package))", worker?: string}
#C: {lint?: string, worker?: string}
_names: ["a", "b"]
jobs: {for n in _names {(n): #C & {worker: "mac"} & _tmpl & {#package: n}}}
