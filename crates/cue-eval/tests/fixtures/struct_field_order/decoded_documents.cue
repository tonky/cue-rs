// Decoded JSON and YAML objects keep the key order of the document.
// Each k* list is the order a comprehension yields the fields in.

import "encoding/json"
import "encoding/yaml"

j1: json.Unmarshal("{\"b\":1,\"a\":{\"z\":1.5,\"y\":[{\"q\":1,\"p\":2}]}}")
j2: yaml.Unmarshal("b: 1\na:\n  z: 1\n  y:\n  - q: 1\n    p: 2\n1: x\n")
kj1: [for k, v in j1 {k}]
kj1a: [for k, v in j1.a {k}]
kj1q: [for k, v in j1.a.y[0] {k}]
kj2: [for k, v in j2 {k}]
kj2a: [for k, v in j2.a {k}]
kj2q: [for k, v in j2.a.y[0] {k}]
