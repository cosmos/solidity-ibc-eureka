#!/bin/bash

# Build the Anchor program
echo "Building Anchor program..."
anchor build

# Check if build was successful
if [ $? -eq 0 ]; then
    echo "Build successful. Copying .so file to program target directory..."
    
    # Create target/deploy directory if it doesn't exist
    mkdir -p target/deploy
    
    # Copy the .so file from root workspace to program directory
    if [ -f "../../target/deploy/ics07_tendermint.so" ]; then
        cp "../../target/deploy/ics07_tendermint.so" "target/deploy/"
        echo "✅ Copied ics07_tendermint.so to target/deploy/"
    else
        echo "❌ Warning: ics07_tendermint.so not found in root workspace target/deploy"
    fi
    
    # Also copy the keypair if it exists in root
    if [ -f "../../target/deploy/ics07_tendermint-keypair.json" ]; then
        cp "../../target/deploy/ics07_tendermint-keypair.json" "target/deploy/"
        echo "✅ Copied ics07_tendermint-keypair.json to target/deploy/"
    fi
else
    echo "❌ Build failed"
    exit 1
fi

echo "Build and setup complete!"
