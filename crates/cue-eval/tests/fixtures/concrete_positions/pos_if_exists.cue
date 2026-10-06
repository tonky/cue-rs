s: {a: 1, b?: 2}
o: {
	if s.a != _|_ {a: true}
	if s.b == _|_ {b: false}
}
