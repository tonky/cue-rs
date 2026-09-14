package repro

import "example.com/stringbounds/schema"

#Policy: {
    name: string & !=""
}
policy: #Policy & {name: "team-policy"}
imported: schema.#Policy & {name: "team-api"}
