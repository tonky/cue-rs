#T: {timeout: =~"^[0-9]+s$", command: string}
task: #T & {command: "t", timeout: 1}
