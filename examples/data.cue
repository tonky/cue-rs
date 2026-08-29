deploy: #Deployment & {
    name: "web-service"
    replicas: 3
    image: "registry.example.com/api:v1.2.0"
    port: 8080
}
