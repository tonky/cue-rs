#Svc: {
	name: string | *"n"
	port: int | *80
	url:  "http://\(name):\(port)"
}
svcs: [string]: #Svc
svcs: s0: {name: "s0", port: 0}
svcs: s1: {name: "s1", port: 1}
