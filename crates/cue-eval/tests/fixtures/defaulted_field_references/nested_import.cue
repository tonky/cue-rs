package repro

import "example.com/defaults/presets"

viaDefault: presets.#Preset
viaOverride: presets.#Preset & {size: 15432}
