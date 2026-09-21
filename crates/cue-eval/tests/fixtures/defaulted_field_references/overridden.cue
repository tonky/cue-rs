package repro

import "example.com/defaults/recipes"

byDefault: recipes.#Recipe
overridden: recipes.#Recipe & {
	size: 15432
	root: "/dev/shm/r"
	who:  "analytics"
}
