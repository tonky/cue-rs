P: {
	base: int | *100
	svcs: [string]: {ports: [...int]}
	svcs: {web: ports: *[1, 2] | [...int]}
	for name, svc in svcs {
		for p in svc.ports {"\(name)_\(p)": p + base}
	}
}
p1: P & {base: 200}
p2: P & {svcs: web: ports: [1, 2]}
p3: P & {svcs: web: ports: [3]}
