#!/bin/bash

# This script generates valid secp256k1 keys in PEM format for testing

set -e

mkdir -p test-keys

# Generate key for attestor 1
echo "Generating key for attestor 1..."
openssl ecparam -name secp256k1 -genkey -noout -out test-keys/attestor1.pem
chmod 600 test-keys/attestor1.pem
echo "Key saved to test-keys/attestor1.pem"

# Generate key for attestor 2
echo "Generating key for attestor 2..."
openssl ecparam -name secp256k1 -genkey -noout -out test-keys/attestor2.pem
chmod 600 test-keys/attestor2.pem
echo "Key saved to test-keys/attestor2.pem"

# Generate key for attestor 3
echo "Generating key for attestor 3..."
openssl ecparam -name secp256k1 -genkey -noout -out test-keys/attestor3.pem
chmod 600 test-keys/attestor3.pem
echo "Key saved to test-keys/attestor3.pem"

echo "All keys generated successfully."
echo "To view a key: cat test-keys/attestor1.pem"
echo "To use these keys, update the docker-compose.yml file with the content of these files." 