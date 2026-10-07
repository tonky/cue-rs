#Hook: string | [...string] | {run: string | [...string], timeout?: string}
#L: {init?: #Hook, postInit?: #Hook}
#P: {
	database: string | *"postgres"
	lifecycle?: #L
	lifecycle: {
		init:     *["initdb"] | #Hook
		postInit: *["[ \"\(database)\" = postgres ] || create \(database)"] | #Hook
	}
}
a: #P
b: #P & {database: "app"}
c: #P & {database: "app", lifecycle: {postInit: "mine", init: ["x", "y"]}}
d: #P & {lifecycle: init: {run: "z", timeout: "1m"}}
