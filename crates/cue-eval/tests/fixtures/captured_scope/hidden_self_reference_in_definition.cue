#D: {
	_L: {"a": ["x"], "b": _L["a"]}
	out: {for k, v in _L {(k): v}}
}
r: #D
