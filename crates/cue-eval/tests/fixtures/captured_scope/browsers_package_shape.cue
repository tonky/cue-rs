// enve pkgs/playwright_browsers.cue: a definition whose let iterates a
// hidden table declared below it, one entry of which reads another.
#Browsers: {
	features: [...string]
	let linux = {
		for name in _tools {(name): true}
		for browser in features for name in _libs[browser] {(name): true}
	}
	buildInputs: [for name, _ in linux {name}]
}
_tools: ["patchelf"]
_libs: {
	"chromium-headless-shell": ["glibc", "nss"]
	"chromium": _libs["chromium-headless-shell"]
	"ffmpeg": ["glibc"]
}
r: #Browsers & {features: ["chromium", "ffmpeg"]}
