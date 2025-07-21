#!/bin/bash

set -e

echo "🧪 Testing IBC Attestor and Sig-Aggregator services..."

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

# Check if services are running
echo "🔍 Checking if services are running..."

# Check container status
for service in ibc-attestor-1 ibc-attestor-2 ibc-attestor-3 sig-aggregator; do
    echo -n "  Checking $service status... "
    if [ "$(docker-compose ps -q $service 2>/dev/null)" == "" ]; then
        echo "❌ NOT RUNNING"
        echo "    Service $service is not running. Please start the services first:"
        echo "    ./scripts/start-agg-services.sh"
        exit 1
    else
        HEALTH_STATUS=$(docker-compose ps -q $service | xargs docker inspect --format='{{.State.Health.Status}}' 2>/dev/null || echo "unknown")
        if [ "$HEALTH_STATUS" = "healthy" ]; then
            echo "✅ RUNNING (healthy)"
        else
            echo "⚠️  RUNNING (status: $HEALTH_STATUS)"
            echo "    Service $service is running but not reported as healthy yet."
            echo "    Check logs with: docker-compose logs $service"
        fi
    fi
done

# Test individual attestors using query
echo ""
echo "🧪 Testing individual attestors..."

for port in 8080 8081 8082; do
    service="ibc-attestor-$((port - 8079))"
    echo -n "  Testing $service on port $port... "

    RESULT=$(grpcurl -plaintext -d '{"height": 100}' localhost:$port ibc_attestor.AttestationService/GetAttestationsFromHeight 2>&1 || echo "FAILED")

    if [[ "$RESULT" == *"FAILED"* ]] || [[ "$RESULT" == *"failed"* ]] || [[ "$RESULT" == *"error"* ]]; then
        echo "❌ FAILED"
        echo "    gRPC call to $service failed"
        echo "    Error: $RESULT"
        echo "    Check logs with: docker-compose logs $service"
    else
        echo "✅ OK"
        echo "    Response received from $service"
    fi
done

# Test the aggregator
echo ""
echo "🧪 Testing sig-aggregator..."
echo -n "  Testing aggregator gRPC functionality... "

RESULT=$(grpcurl -plaintext -d '{"min_height": 100}' localhost:50060 aggregator.Aggregator.GetAggregateAttestation 2>&1 || echo "FAILED")

if [[ "$RESULT" == *"FAILED"* ]] || [[ "$RESULT" == *"failed"* ]] || [[ "$RESULT" == *"error"* ]]; then
    echo "❌ FAILED"
    echo "    gRPC call to aggregator failed"
    echo "    Error: $RESULT"
    echo "    Check logs with: docker-compose logs sig-aggregator"

    # Check for transport errors
    if [[ "$RESULT" == *"transport error"* ]]; then
        echo ""
        echo "⚠️  Transport errors detected. This might indicate connectivity issues between the aggregator and attestors."
        echo "    Run the debug script for more information: ./programs/sig-aggregator/debug-transport.sh"
    fi
else
    echo "✅ OK"
    echo "    Response received from aggregator"
    echo "    Sample response: $(echo "$RESULT" | head -5)..."
fi

echo ""
echo "🎉 Service testing completed!"
echo ""
echo "📋 Test Summary:"
echo "  • Attestor services: Running on ports 8080, 8081, 8082"
echo "  • Aggregator service: Running on port 50060"
echo ""
echo "💡 For JSON formatted output:"
echo "  grpcurl -plaintext -d '{\"min_height\": 100}' localhost:50060 aggregator.Aggregator.GetAggregateAttestation | jq"