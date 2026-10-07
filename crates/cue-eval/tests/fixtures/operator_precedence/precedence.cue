// Every value here depends on the parentheses or the precedence ladder: a formatter that
// drops a pair the tree needs, or a parser that ranks an operator differently from the
// spec, changes one of them.
#A: {
	raw: string
	out: raw + "!"
}

#B: {n: int, sq: n * n}

selectOfUnification: (#A & {raw: "a"}).out
indexOfDisjunction: (*[1, 2] | [3])[1]
selectOfDefinition: (#B & {n: 3}).sq
sliceOfUnification: ([1, 2, 3] & [1, 2, 3])[1:3]
quotedSelector: {"b-c": 5, b: 1, c: 1}."b-c"
negatedNegation:   -(-1)
negatedSum:        -(1 + 2)
differenceOfDiff:  10 - (4 - 1)
quotientOfProduct: 12 / (2 * 3)
productOfSum:      (1 + 2) * 3

// `||` and `&&` bind tighter than `&` and `|`.
logicalUnderDisjunction: false && false | *true
logicalUnderUnification: (1+1 == 2 || false) & bool
unificationUnderLogical: true || (false & bool)
disjunctionUnderLogical: false || (*true | false)
