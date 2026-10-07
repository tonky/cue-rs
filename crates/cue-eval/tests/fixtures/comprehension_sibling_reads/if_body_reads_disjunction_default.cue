P: {
	mode: *"x" | "y"
	if true {label: "mode-\(mode)"}
	if mode == "x" {isX: true}
}
p1: P & {mode: "y"}
