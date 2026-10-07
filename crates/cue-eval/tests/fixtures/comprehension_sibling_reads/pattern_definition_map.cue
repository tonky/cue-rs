#Svc: {
	name: string | *"n"
	port: int | *80
	url:  "http://\(name):\(port)"
}
svcs: [string]: #Svc
svcs: {
	a: {name: "a"}
	b: {name: "b", port: 90}
	c: {}
}
