#Hook: string | [...string] | {run: string | [...string], timeout?: string}
#L: {init?: #Hook, postInit?: #Hook}
#P: {
	database: string | *"postgres"
	lifecycle?: #L
	lifecycle: {
		postInit: *[for d in [database] if d != "postgres" {"create \(d)"}] | #Hook
	}
}
a: #P
b: #P & {database: "app"}
c: #P & {database: "app", lifecycle: {postInit: "mine"}}
