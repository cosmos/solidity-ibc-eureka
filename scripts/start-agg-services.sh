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

# Stop any existing services
echo "🛑 Stopping any existing services..."
docker-compose down

# Clean up volumes to ensure fresh keys
echo "🧹 Cleaning up volumes..."
docker volume rm -f sig-aggregator_attestor1_keys sig-aggregator_attestor2_keys sig-aggregator_attestor3_keys 2>/dev/null || true

echo "📦 Building and starting services..."
docker-compose up --build -d

echo "⏳ Waiting for services to become ready..."

# Wait for services to be healthy with a timeout
MAX_WAIT_SECONDS=120
START_TIME=$(date +%s)

for service in ibc-attestor-1 ibc-attestor-2 ibc-attestor-3 sig-aggregator; do
    echo "⏳ Waiting for $service to be healthy..."
    
    while true; do
        # Check if the container is running
        if ! docker-compose ps -q $service > /dev/null 2>&1; then
            echo "❌ Service $service is not running. Checking logs..."
            docker-compose logs $service
            echo "❌ Service failed to start. Please check the logs above for errors."
            exit 1
        fi
        
        # Get health status
        HEALTH_STATUS=$(docker-compose ps -q $service | xargs docker inspect --format='{{.State.Health.Status}}' 2>/dev/null || echo "unknown")
        
        # Check for timeout
        CURRENT_TIME=$(date +%s)
        ELAPSED_TIME=$((CURRENT_TIME - START_TIME))
        
        if [ $ELAPSED_TIME -gt $MAX_WAIT_SECONDS ]; then
            echo "❌ Timeout waiting for $service to become healthy. Checking logs..."
            docker-compose logs $service
            echo "❌ Service health check timed out. Please check the logs above for errors."
            exit 1
        fi
        
        if [ "$HEALTH_STATUS" = "healthy" ]; then
            echo " ✅ $service is healthy"
            break
        else
            echo -n "."
            sleep 2
        fi
    done
done

echo ""
echo "🎉 All services are up and running!"
echo ""
echo "Service URLs:"
echo "  • IBC Attestor 1: gRPC on localhost:8080"
echo "  • IBC Attestor 2: gRPC on localhost:8081" 
echo "  • IBC Attestor 3: gRPC on localhost:8082"
echo "  • Sig-Aggregator: gRPC on localhost:50060"
echo ""
echo "Test the setup with:"
echo "  ./scripts/test-agg-services.sh"
echo ""
echo "Make a direct gRPC request:"
echo "  grpcurl -plaintext -d '{\"min_height\": 100}' localhost:50060 aggregator.Aggregator.GetAggregateAttestation | jq"
echo ""
echo "View logs with:"
echo "  cd programs/sig-aggregator && docker-compose logs -f"
echo ""
echo "Stop services with:"
echo "  cd programs/sig-aggregator && docker-compose down"