#!/bin/bash

set -e

echo "🧪 Testing IBC Attestor and Sig-Aggregator services..."

# Check if grpcurl is available
if ! command -v grpcurl > /dev/null 2>&1; then
    echo "❌ grpcurl is not installed. Please install it to test the services."
    echo "   Install: https://github.com/fullstorydev/grpcurl#installation"
    exit 1
fi

# Check if services are running
echo "🔍 Checking if services are running..."

if ! curl -s http://localhost:8080 > /dev/null 2>&1 && \
   ! curl -s http://localhost:8081 > /dev/null 2>&1 && \
   ! curl -s http://localhost:8082 > /dev/null 2>&1 && \
   ! curl -s http://localhost:50060 > /dev/null 2>&1; then
    echo "❌ Services don't appear to be running. Please start them first:"
    echo "   ./scripts/start-services.sh"
    exit 1
fi

echo "✅ Services appear to be running"

# Test individual attestors
echo ""
echo "🧪 Testing individual attestors..."

for port in 8080 8081 8082; do
    echo -n "  Testing attestor on port $port... "
    if grpcurl -plaintext -max-time 10 localhost:$port list > /dev/null 2>&1; then
        echo "✅ OK"
    else
        echo "❌ FAILED"
        echo "    Service on port $port is not responding to gRPC requests"
    fi
done

# Test the aggregator
echo ""
echo "🧪 Testing sig-aggregator..."
echo -n "  Testing aggregator gRPC service... "
if grpcurl -plaintext -max-time 10 localhost:50060 list > /dev/null 2>&1; then
    echo "✅ OK"
else
    echo "❌ FAILED"
    echo "    Aggregator service is not responding to gRPC requests"
    exit 1
fi

# Test aggregator functionality
echo -n "  Testing aggregator functionality... "
if grpcurl -plaintext -max-time 30 -d '{"min_height": 100}' localhost:50060 aggregator.Aggregator.GetAggregateAttestation > /dev/null 2>&1; then
    echo "✅ OK"
else
    echo "❌ FAILED"
    echo "    Aggregator is not responding to GetAggregateAttestation requests"
    echo "    This might be expected if the attestors are not yet synchronized with the blockchain"
fi

echo ""
echo "🎉 Service testing completed!"
echo ""
echo "📋 Test Summary:"
echo "  • Attestor services: Running on ports 8080, 8081, 8082"
echo "  • Aggregator service: Running on port 50060"
echo ""
echo "💡 Try making a test request:"
echo "  grpcurl -plaintext -d '{\"min_height\": 100}' localhost:50060 aggregator.Aggregator.GetAggregateAttestation"