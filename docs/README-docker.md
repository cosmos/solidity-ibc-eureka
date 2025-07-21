# IBC Attestor and Sig-Aggregator Docker Setup

This Docker Compose setup spins up three separate IBC attestor servers and one sig-aggregator that uses those attestors to provide aggregated attestations.

## Architecture

- **3 IBC Attestor Instances**: Running on ports 8080, 8081, and 8082
- **1 Sig-Aggregator**: Running on port 50060, configured with a quorum threshold of 2

## Prerequisites

- Docker and Docker Compose installed
- At least 4GB of available RAM (for building the Rust binaries)

## Usage

### Starting the Services

```bash
# Option 1: Use the startup script (recommended)
./scripts/start-services.sh

# Option 2: Manual Docker Compose commands
cd programs/sig-aggregator
docker-compose up --build -d
```

### Stopping the Services

```bash
# Stop all services
cd programs/sig-aggregator
docker-compose down

# Stop and remove volumes
docker-compose down -v
```

### Service URLs

- IBC Attestor 1: `localhost:8080`
- IBC Attestor 2: `localhost:8081` 
- IBC Attestor 3: `localhost:8082`
- Sig-Aggregator: `localhost:50060`

### Testing the Setup

You can test the services using grpcurl:

#### Test individual attestors:
```bash
# Test attestor 1
grpcurl -plaintext -d '{"height": 100}' localhost:8080 ibc_attestor.AttestationService/GetAttestationsFromHeight

# Test attestor 2  
grpcurl -plaintext -d '{"height": 100}' localhost:8081 ibc_attestor.AttestationService/GetAttestationsFromHeight

# Test attestor 3
grpcurl -plaintext -d '{"height": 100}' localhost:8082 ibc_attestor.AttestationService/GetAttestationsFromHeight
```

#### Test the aggregator:
```bash
# Get aggregated attestation
grpcurl -plaintext -d '{"min_height": 100}' localhost:50060 aggregator.Aggregator.GetAggregateAttestation
```

### Configuration

The configuration files are located in the `programs/sig-aggregator/config/` directory:

- `programs/sig-aggregator/config/attestor1.toml` - Configuration for IBC Attestor 1
- `programs/sig-aggregator/config/attestor2.toml` - Configuration for IBC Attestor 2  
- `programs/sig-aggregator/config/attestor3.toml` - Configuration for IBC Attestor 3
- `programs/sig-aggregator/config/aggregator.toml` - Configuration for the Sig-Aggregator

### Customizing the Setup

#### Changing Quorum Threshold

Edit `programs/sig-aggregator/config/aggregator.toml` and modify the `quorum_threshold` value:

```toml
[attestor]
quorum_threshold = 3  # Require all 3 attestors to agree
```

#### Adding More Attestors

1. Add a new service to `programs/sig-aggregator/docker-compose.yml` following the pattern of existing attestors
2. Create a new configuration file in `programs/sig-aggregator/config/`
3. Add the new endpoint to `programs/sig-aggregator/config/aggregator.toml`

#### Using Different Solana Networks

Edit the attestor configuration files and change the `url` in the `[solana]` section:

```toml
[solana]
url = "https://api.mainnet-beta.solana.com"  # For mainnet
# or
url = "https://api.testnet.solana.com"       # For testnet
```

## Logs

View logs for all services:
```bash
cd programs/sig-aggregator
docker-compose logs -f
```

View logs for a specific service:
```bash
cd programs/sig-aggregator
docker-compose logs -f ibc-attestor-1
docker-compose logs -f sig-aggregator
```

## Health Checks

The services include health checks that verify they're responding to gRPC requests. You can check the health status:

```bash
cd programs/sig-aggregator
docker-compose ps
```

Healthy services will show status as "Up" with "(healthy)" indicator.

## Troubleshooting

### Port Conflicts
If you have port conflicts, modify the ports in `programs/sig-aggregator/docker-compose.yml`:

```yaml
ports:
  - "9080:8080"  # Use port 9080 instead of 8080
```

### Build Issues
If you encounter build issues:

1. Clean up Docker resources:
```bash
cd programs/sig-aggregator
docker-compose down -v
docker system prune -f
```

2. Rebuild from scratch:
```bash
cd programs/sig-aggregator
docker-compose build --no-cache
```

### Service Not Starting
Check the logs:
```bash
cd programs/sig-aggregator
docker-compose logs [service-name]
```

Common issues:
- Missing dependencies in Dockerfile
- Configuration file errors
- Port binding issues
- Insufficient resources