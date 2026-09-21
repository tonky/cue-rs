package recipes

// A definition whose fields read other fields of the same definition, each carrying a
// disjunction default. An importer overrides the inputs; every reader must follow.
#Recipe: {
	size:  (>0 & <65536) | *5432
	root:  string | *"/var/lib"
	who:   string | *"nobody"

	label: string | *"run -r \(root) -n \(size)"
	copy:  size
	inner: tag: "\(who)@\(size)"
}
