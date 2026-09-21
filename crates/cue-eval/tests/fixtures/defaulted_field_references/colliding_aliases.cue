package repro

import (
	"example.com/defaults/presets"
	"example.com/defaults/alt/naming"
)

// Both this file and `presets` bind the identifier `naming`, to different
// packages.
hostPrefix: naming.prefix
overridden: presets.#Preset & {size: 15432}
