package schema

#Stage: {
	name: string
	needs?: [...#Stage]
}

#Pipeline: {
	stages: [string]: #Stage
}
