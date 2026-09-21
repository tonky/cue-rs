package stacks

import "example.com/defaults/presets"

// Two boundaries away from whoever overrides `size`: stacks -> presets -> naming.
#Stack: {
	preset:  presets.#Preset
	summary: string | *"\(preset.label)/\(preset.copy)"
}
