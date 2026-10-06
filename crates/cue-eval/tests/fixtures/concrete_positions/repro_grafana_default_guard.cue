
let items = {
	[string]: {flag: *true | bool}
	a: {}
	b: {flag: false}
}
out: {
	for name, i in items {
		(name): {
			if i.flag {
				on: true
			}
			always: true
		}
	}
}
