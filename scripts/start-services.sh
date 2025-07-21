#!/bin/bash

set -e

echo "🚀 Starting IBC Attestor and Sig-Aggregator services..."

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo "❌ Docker is not running. Please start Docker and try again."
    exit 1
fi

# Check if docker-compose is available
if ! command -v docker-compose > /dev/null 2>&1; then
    echo "❌ docker-compose is not installed. Please install it and try again."
    exit 1
fi

# Navigate to the sig-aggregator directory
cd "$(dirname "$0")/../programs/sig-aggregator"

# Create config directory if it doesn't exist
mkdir -p config

echo "📦 Building and starting services..."
docker-compose up --build -d

echo "⏳ Waiting for services to become healthy..."
sleep 10

# Wait for services to be healthy
for service in ibc-attestor-1 ibc-attestor-2 ibc-attestor-3 sig-aggregator; do
    echo "⏳ Waiting for $service to be healthy..."
    while [ "$(docker-compose ps -q $service | xargs docker inspect --format='{{.State.Health.Status}}')" != "healthy" ]; do
        sleep 2
        echo -n "."
    done
    echo " ✅ $service is healthy"
done

echo ""
echo "🎉 All services are up and running!"
echo ""
echo "Service URLs:"
echo "  • IBC Attestor 1: http://localhost:8080"
echo "  • IBC Attestor 2: http://localhost:8081" 
echo "  • IBC Attestor 3: http://localhost:8082"
echo "  • Sig-Aggregator: http://localhost:50060"
echo ""
echo "Test the setup with:"
echo "  grpcurl -plaintext -d '{\"min_height\": 100}' localhost:50060 aggregator.Aggregator.GetAggregateAttestation"
echo ""
echo "View logs with:"
echo "  cd programs/sig-aggregator && docker-compose logs -f"
echo ""
echo "Stop services with:"
echo "  cd programs/sig-aggregator && docker-compose down"