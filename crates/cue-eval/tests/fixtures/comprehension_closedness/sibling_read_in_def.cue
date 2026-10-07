#P: {
	database: string | *"postgres"
	if database != "" {
		postInit: "create \(database)"
	}
}
b: #P & {database: "app"}
