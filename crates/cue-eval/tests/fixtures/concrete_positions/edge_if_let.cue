_items: {a: {f: *true | bool}}
o: { for k, i in _items let g = i.f if g { (k): 1 } }
