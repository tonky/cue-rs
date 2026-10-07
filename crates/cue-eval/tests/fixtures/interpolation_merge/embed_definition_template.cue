#Task: {name?: string, command: string}
#C: {worker?: string, lint?: string | #Task, #package?: string, ...}
#OnMac: {worker: "mac", ...}
#Tmpl: {
	#C
	#OnMac
	#package: string
	lint: {name: "clippy", command: "cargo clippy -p \(#package)"}
}
x: #Tmpl & {#package: "x"}
