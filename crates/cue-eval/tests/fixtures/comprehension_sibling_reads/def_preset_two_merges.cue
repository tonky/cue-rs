#P: {
	database: string | *"postgres"
	user:     string | *"admin"
	if database != "" {
		postInit: "create \(database) owner \(user)"
	}
}
b: #P & {database: "app"}
c: b & {user: "bob"}
