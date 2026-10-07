// enve pkgs/playwright_browsers.cue: a feature with no build of its version.
_builds: {"1": {a: {archives: {s: {url: "u"}}}}}
#Browsers: {
	version: string
	features: [...string]
	let builds = _builds[version]
	fetch: {
		for browser in features {
			(browser): url: {for system, archive in builds[browser].archives {(system): archive.url}}
		}
	}
}
r: #Browsers & {version: "1", features: ["a", "z"]}
