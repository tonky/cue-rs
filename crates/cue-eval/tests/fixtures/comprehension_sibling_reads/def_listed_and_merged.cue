#Svc: {
	name: string
	port: int | *80
	if true {url: "http://\(name):\(port)"}
}
svcs: [...#Svc]
svcs: [{name: "a"}, {name: "b", port: 90}]
one: #Svc & {name: "c"} & {port: 1}
