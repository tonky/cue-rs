// Comprehensions are elements of the list literal that holds them: a list
// made only of a comprehension, nested in another list, stays a list.

import "list"

x: [[1], [for k in [2, 3] {k}]]
y: [[for k in [2, 3] {k}]]
z: [[for k in [2, 3] {k}, 4]]
deep: [[[for k in [1, 2] {k}]]]
deeper: [[[[for k in [1] {k}]], [for k in [2] {k}]]]
mixed: [0, for k in [1, 2] {k}, 3, for k in [4] {k}]
mixedNested: [0, [for k in [1, 2] {k}], for k in [3] {[k]}]
guard: [[if true {1}], [if false {1}], [if true {2}, 3]]
onlyIf: [[if true {1}]]
twoComps: [[for k in [1] {k}, for k in [2] {k}]]
nestedFor: [[for i in [1, 2] for j in [10, 20] {i * j}]]
nestedComp: [for i in [1, 2] {[for j in [i, i + 1] {j}]}]
nestedComp2: [[for i in [1, 2] {[for j in [i] {j}]}]]
structs: [[for k in ["a", "b"] {name: k}]]
structs2: [for k in ["a"] {name: k}, [for k in ["b"] {name: k}]]
lists: [[for k in [1, 2] {[k, k]}]]
letClause: [[for k in [1, 2] let d = k * 2 {d}]]
empty: [[for k in [] {k}], []]
emptyIf: [[if false {1}]]
inStruct: {a: [for k in [1] {k}], b: [[for k in [1] {k}]]}
concat: list.Concat([[1], [for k in [2, 3] {k}]])
concat2: list.Concat([[for k in [1] {k}], [for k in [2, 3] {k}]])
flatten: list.FlattenN([[for k in [1] {[k]}]], 1)
len1: len([[for k in [1, 2] {k}]])
indexed: [[for k in [1, 2] {k}]][0][1]
withEllipsis: [[for k in [1] {k}, ...int]] & [[1, 2]]
unify: [[for k in [1] {k}]] & [[1]]
struct2: {for k in ["p"] {(k): [[for j in [1] {j}]]}}
