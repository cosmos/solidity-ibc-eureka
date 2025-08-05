# GitHub Workflow for Testing local_node.sh

This repository contains a GitHub Actions workflow that automatically tests the `local_node.sh` script to ensure it works correctly in a CI/CD environment.

## Overview

The workflow file (`.github/workflows/test-local-node.yml`) provides comprehensive testing for the `local_node.sh` script, including:

- **Multi-OS Testing**: Tests on multiple Ubuntu versions
- **Dependency Management**: Installs and caches all required dependencies
- **Script Validation**: Checks syntax and best practices
- **Node Health Checks**: Verifies the node starts and operates correctly
- **Endpoint Testing**: Tests RPC, EVM RPC, gRPC, and API endpoints
- **Error Handling**: Captures logs and debug information on failures
- **Cleanup**: Ensures proper resource cleanup after tests

## Workflow Triggers

The workflow runs automatically when:

1. **Push to main branches**: When changes are pushed to `main`, `master`, or `develop` branches
2. **Pull Requests**: When PRs are opened or updated against the main branches
3. **Manual Trigger**: Can be manually triggered via GitHub Actions UI
4. **Path-based**: Only runs when relevant files are changed:
   - `local_node.sh`
   - `.github/workflows/test-local-node.yml`
   - Files in `scripts/` or `config/` directories

## Workflow Jobs

### 1. test-local-node

This job runs the main testing suite:

- **Environment Setup**: Installs Go, Node.js, and system dependencies
- **Script Execution**: Runs `local_node.sh` with timeout protection
- **Health Checks**: Verifies node is producing blocks
- **Endpoint Tests**: Tests all exposed endpoints (RPC, EVM, gRPC)
- **Log Analysis**: Checks logs for errors or issues
- **Artifact Upload**: Saves logs for debugging

### 2. validate-script-best-practices

This job validates script quality:

- **ShellCheck**: Runs static analysis on the shell script
- **Documentation Check**: Verifies the script has proper documentation
- **Error Handling Check**: Ensures the script has proper error handling

## How to Use

### For Repository Maintainers

1. **Add the workflow** to your repository:
   ```bash
   cp .github/workflows/test-local-node.yml <your-repo>/.github/workflows/
   ```

2. **Ensure your local_node.sh** is in the repository root

3. **Configure environment variables** in the workflow if needed:
   - `GO_VERSION`: Go version to use (default: 1.21)
   - `NODE_VERSION`: Node.js version (default: 18)
   - `TIMEOUT_MINUTES`: Maximum runtime (default: 30)

### For Contributors

When submitting PRs that modify `local_node.sh`:

1. **Ensure tests pass locally**:
   ```bash
   ./local_node.sh
   ```

2. **Check script syntax**:
   ```bash
   bash -n local_node.sh
   ```

3. **Run ShellCheck** (if installed):
   ```bash
   shellcheck local_node.sh
   ```

## Expected Behavior

The workflow expects `local_node.sh` to:

1. **Start within 5 minutes**: The node should be responsive on port 26657
2. **Produce blocks**: The node should start producing blocks (height > 0)
3. **Expose standard endpoints**:
   - Tendermint RPC: Port 26657
   - EVM JSON-RPC: Port 8545 (or similar)
   - gRPC: Port 9090
   - REST API: Port 1317

## Troubleshooting

### Common Issues

1. **Port conflicts**: The workflow checks for port availability and waits if needed
2. **Timeout errors**: Increase the timeout in the workflow if your node takes longer to start
3. **Missing dependencies**: The workflow installs common dependencies, but you may need to add more

### Debugging Failed Runs

1. **Check the workflow logs** in the Actions tab
2. **Download artifacts**: Failed runs upload logs as artifacts
3. **Review debug information**: The workflow collects system info on failures

## Customization

### Adding More Tests

To add additional tests, modify the "Run integration tests" step:

```yaml
- name: Run integration tests
  run: |
    # Add your custom tests here
    # Example: Test specific EVM functionality
    curl -X POST http://localhost:8545 \
      -H "Content-Type: application/json" \
      -d '{"jsonrpc":"2.0","method":"eth_accounts","params":[],"id":1}'
```

### Changing Ports

If your node uses different ports, update the port checks:

```yaml
# In the workflow file, update these sections:
# - "Check node health" step
# - "Test EVM RPC endpoints" step
# - "Stop node" step
```

### Adding Dependencies

To add more system dependencies:

```yaml
- name: Install system dependencies
  run: |
    sudo apt-get update
    sudo apt-get install -y \
      # ... existing dependencies ...
      your-new-dependency
```

## Sample local_node.sh

A reference implementation of `local_node.sh` is provided that includes:

- **Comprehensive error handling**
- **Port availability checks**
- **Colored output for better readability**
- **Command-line argument parsing**
- **Automatic cleanup on exit**
- **Health check verification**

## Best Practices

1. **Always use error handling**: Set `set -e` at the beginning of your script
2. **Check dependencies**: Verify required tools are installed
3. **Handle cleanup**: Use trap to ensure cleanup on exit
4. **Provide feedback**: Use colored output to indicate progress
5. **Support configuration**: Allow environment variables for customization
6. **Document usage**: Include help text and examples

## Contributing

When contributing to this workflow:

1. **Test locally first**: Ensure your changes work in your environment
2. **Update documentation**: Keep this README up to date
3. **Consider compatibility**: Ensure changes work on all supported OS versions
4. **Add tests**: Include tests for new functionality

## Security Considerations

- The workflow runs with limited permissions
- Sensitive data should not be logged
- Use GitHub Secrets for any credentials
- The test keyring backend is used (not for production)

## Resources

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Cosmos SDK Documentation](https://docs.cosmos.network/)
- [Evmos Documentation](https://docs.evmos.org/)
- [ShellCheck](https://www.shellcheck.net/)

## License

This workflow is provided as-is for testing purposes. Adjust according to your project's license.