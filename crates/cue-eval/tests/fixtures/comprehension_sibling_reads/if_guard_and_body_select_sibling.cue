P: {
	cfg: {enabled: bool | *true, name: string | *"d"}
	if cfg.enabled {x: cfg.name}
}
p1: P & {cfg: name: "z"}
p2: P & {cfg: enabled: false}
