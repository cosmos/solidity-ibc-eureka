#!/bin/bash

# Script to test the GitHub workflow locally using act
# https://github.com/nektos/act

set -e

echo "=== Local GitHub Workflow Testing Script ==="
echo ""

# Check if act is installed
if ! command -v act &> /dev/null; then
    echo "❌ 'act' is not installed."
    echo ""
    echo "To install act, run one of the following:"
    echo "  - macOS: brew install act"
    echo "  - Linux: curl https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash"
    echo "  - Or download from: https://github.com/nektos/act/releases"
    echo ""
    exit 1
fi

echo "✅ act is installed: $(act --version)"
echo ""

# Check if Docker is running
if ! docker info &> /dev/null; then
    echo "❌ Docker is not running. Please start Docker first."
    exit 1
fi

echo "✅ Docker is running"
echo ""

# Create a temporary event file for manual trigger
cat > /tmp/workflow_dispatch.json << EOF
{
  "action": "workflow_dispatch",
  "workflow": ".github/workflows/test-local-node.yml"
}
EOF

echo "Available workflows to test:"
echo "1. test-local-node (full test)"
echo "2. validate-script-best-practices (validation only)"
echo ""

read -p "Which job would you like to test? (1 or 2): " choice

case $choice in
    1)
        echo ""
        echo "Running test-local-node job..."
        echo "This will:"
        echo "  - Set up the environment"
        echo "  - Run local_node.sh"
        echo "  - Perform health checks"
        echo "  - Clean up resources"
        echo ""
        
        # Run the specific job
        act workflow_dispatch \
            -j test-local-node \
            -P ubuntu-latest=catthehacker/ubuntu:act-latest \
            -P ubuntu-22.04=catthehacker/ubuntu:act-22.04 \
            --eventpath /tmp/workflow_dispatch.json \
            --verbose
        ;;
    
    2)
        echo ""
        echo "Running validate-script-best-practices job..."
        echo "This will:"
        echo "  - Install shellcheck"
        echo "  - Validate script syntax"
        echo "  - Check for best practices"
        echo ""
        
        # Run the validation job
        act workflow_dispatch \
            -j validate-script-best-practices \
            -P ubuntu-latest=catthehacker/ubuntu:act-latest \
            --eventpath /tmp/workflow_dispatch.json
        ;;
    
    *)
        echo "Invalid choice. Please run the script again and select 1 or 2."
        exit 1
        ;;
esac

# Clean up
rm -f /tmp/workflow_dispatch.json

echo ""
echo "=== Test Complete ==="
echo ""
echo "Note: Some features may behave differently in act compared to GitHub Actions:"
echo "  - Caching might not work"
echo "  - Some GitHub-specific features may be limited"
echo "  - Resource constraints may differ"
echo ""
echo "For the most accurate results, test on a feature branch in GitHub."