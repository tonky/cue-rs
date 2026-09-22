package app

import "example.com/selfref/schema"

pipeline: schema.#Pipeline & {
	stages: build: name: "build"
	stages: test: {
		name: "test"
		needs: [pipeline.stages.build]
	}
}
