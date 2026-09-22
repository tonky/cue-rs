package app

import "example.com/width/schema"

// Every file contributes its own conjunct to the same field, which is what
// puts the service disjunction through one more unification per file.
pipeline: schema.#Pipeline & {
	tasks: "one": "one"
}
