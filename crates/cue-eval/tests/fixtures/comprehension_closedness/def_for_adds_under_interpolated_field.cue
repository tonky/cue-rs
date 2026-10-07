#G: {
	class: string
	objects: Service: "\(class)-x": {spec: 1}
	for name, _ in objects.Service {
		objects: Service: "\(name)": labels: {c: class}
	}
}
a: #G & {class: "admin"}
