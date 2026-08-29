deploy: #Deployment & {
    name: "web-service"
    replicas: 150 // violates <= 100
    image: "invalid image format!"
    port: 8080
}
