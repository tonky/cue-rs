d: *["a", "b"] | [...string]
o: { for x in d { (x): true } }
