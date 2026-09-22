package app

import "example.com/width/schema"

pipeline: schema.#Pipeline & {
	name: "width"
	services: db: {name: "db", kind: "postgres"}
}
