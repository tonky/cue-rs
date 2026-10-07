#Svc: {
	name: string | *"n"
	port: int | *80
	if port != 0 {url: "http://\(name):\(port)"}
	for i, r in [1, 2] {"r\(i)": "\(name)-\(r)"}
}
envs: [string]: [string]: #Svc
envs: {
	dev: a: {name: "a"}
	prod: {
		a: {name: "a", port: 443}
		b: {}
	}
}
