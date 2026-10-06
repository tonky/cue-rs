import "strings"
s: *"a,b" | string
o: strings.Split(s, ",")
p: len(s)
