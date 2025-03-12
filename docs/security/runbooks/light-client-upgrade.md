## RUNBOOK - upgrading light client

### Roles

| Role         | Person |
|--------------|--------|
| Facilitator  |        |
| Communicator |        |
| Notekeeper   |        |
| Signers      |        |

### Runbook

1. Create an operations report (copy of this document) in docs/security/operations 
2. Facilitator determines the correct paramaters for the new light client
    - These can be fetched and independently verified using the `operator` CLI tool.
3. Facilitator creates a pull request in `solidity-ibc-eureka` to add the new light client
4. Signers verify the parameters for the new light client
5. The pull request is merged and the new light client should automatically be deployed using the CI pipeline
    - If the GitHub or the CI pipeline is unavailable, the facilitator should opt to deploy the light client using `forge script DeploySP1TendermintLightClient.sol` script.
6. The signers should independently verify that the Light Client parameters are correct on-chain
    - This can be done by running `DEPLOYMENT_ENV=mainnet-prod forge script scripts/deployments/DeploySP1TendermintLightClient.sol --rpc-url <RPC>`. If the script runs successfully and the parameters are in the `deployments/mainnet-prod/<chain_id>.json` file, this means that the parameters are correct.
7. The facilitator submits a transaction proposal to the Gnosis Safe.
    - The transaction proposal should contain a transaction to the `TimelockController` calling `schedule(ICS26RouterAddress, 0, migrateClient(oldClient, newClientId), 0, 0, 259200)`
    - The `ICS26Router` can be verified in the deployment JSON files, as it should be calling the canonical one.
8. The signers independently verify that the transaction contents contain the expected call
9.  The facilitator collects signatures from the signers on Gnosis Safe
    - ** (!!) The signers should verify the Tenderly simulation from the Gnosis Safe UI. They should make sure that the domainHash matches what they are seeing in the blind-signing window on their hardware wallet**
    - For additional security, the signers should also independently verify the transaction contents by using the [safe-tx-hashes-util](https://github.com/pcaversaccio/safe-tx-hashes-util) tool.
10. The facilitator executes the time-locked transaction on-chain
11. The facilitator executes the transaction in the time-lock on-chain

### Checklist

- [ ] The facilitator has determined the correct parameters for the new light client
- [ ] The facilitator has created a pull request to add the new light client
- [ ] The signers have independently verified that the new light client parameters are correct
- [ ] The facilitator has merged the pull request and the new light client has been deployed on-chain
- [ ] The signers and facilitator have independently verified that the new light client parameters are correct on-chain
- [ ] The facilitator has submitted a time-locked transaction proposal to the Gnosis Safe to migrate light clients
- [ ] The signers have independently verified that the transaction proposal is correct
- [ ] The signers have signed the transaction proposal for the time-lock
- [ ] The facilitator has executed the transaction proposal for the time-lock
- [ ] The facilitator has executed the transaction on-chain