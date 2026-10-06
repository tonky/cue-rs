_items: [{f: *true | bool}, {f: false}]
o: [ for i in _items if i.f { 1 } ]
