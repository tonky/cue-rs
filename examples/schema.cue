#Deployment: {
    name: string
    replicas: int & >0 & <=100
    image: string & =~ "^[a-z0-9/._-]+:[a-z0-9._-]+$"
    port: int & >0 & <65535
    env: *"production" | "staging" | "development"
}
