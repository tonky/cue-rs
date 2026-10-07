#P: {
	_db: string | *"postgres"
	if _db != "" {postInit: "create \(_db)"}
}
b: #P & {_db: "app"}
