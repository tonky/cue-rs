#T: {o: {timeout: int}}
task: #T & {o: timeout: "x"}
y: [...#T]
y: [{o: timeout: "z"}]
