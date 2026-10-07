#Hook: string | [...string]
#L: {init?: #Hook, postInit?: #Hook}
#S: {lifecycle?: #L, database?: string, ...}
#P: #S & {
	database: string | *"postgres"
	lifecycle: init: *["initdb"] | #Hook
	if database != "postgres" {
		lifecycle: postInit: *["create \(database)"] | #Hook
	}
}
a: #P
b: #P & {database: "app"}
c: #P & {database: "app", lifecycle: postInit: "mine"}
