package presets

import "example.com/defaults/naming"

// A definition whose recipe reads a package *this file* imports. Deriving it
// again at an importer's override has to resolve `naming` the way this file
// binds it, not the way the importer does.
#Preset: {
	size:  int | *5432
	label: string | *"\(naming.prefix)-\(size)"
	copy:  size
}
