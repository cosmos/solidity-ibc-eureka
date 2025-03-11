## RUNBOOK - upgrading IBCERC20 contract

### Roles

| Role         | Person |
|--------------|--------|
| Facilitator  |        |
| Communicator |        |
| Notekeeper   |        |
| Signers      |        |

### Runbook

1. Create an operations report in docs/security/operations using the TEMPLATE.md
2. Facilitator deploys a new version of the patched IBCERC20 contract on-chain and verifies it on Etherscan
3. ? Signers independently verify that the deployed bytecode matches the patched IBCERC20 contract
4. The facilitator submits a transaction proposal to the Gnosis Safe.
    - The transaction proposal should contain a transaction to the `TimelockController` calling `schedule(ICS20TransferAddress, 0, upgradeIBCERC20To(newIBCERC20Address), 0, 0, 259200)`
    - The `ICS20Transfer` can be verified in the deployment JSON files, as it should be calling the canonical one.
    - The `TimelockController` address can be verified by looking at the canonical deployment file (`timelockAdmin` field in `ICS26Router`)
    - The calldata to the timelock (field in the schedule call) can be verified by running `cast decode-calldata "upgradeIBCERC20To(address)" <calldata>`
5. The signers independently verify that the transaction contents contain the expected call
6.  The facilitator collects signatures from the signers on Gnosis Safe
    - ** (!!) The signers should verify the Tenderly simulation from the Gnosis Safe UI. They should make sure that the domainHash matches what they are seeing in the blind-signing window on their hardware wallet**
    - For additional security, the signers should also independently verify the transaction contents by using the [safe-tx-hashes-util](https://github.com/pcaversaccio/safe-tx-hashes-util) tool.
7. The facilitator collects signatures from the signers on Gnosis Safe
8. The facilitator executes the time-locked transaction on-chain
9. The facilitator executes the transaction in the time-lock on-chain
10. The facilitator submits and merges a pull request to `solidity-ibc-eureka` to update the new canonical Escrow deployment address. 

### Checklist

- [ ] The facilitator has deployed a new version of the patched IBCERC20 contract on-chain
- [ ] The signers have independently verified that the new contract deployment is correct
- [ ] The facilitator has submitted a time-locked transaction proposal to the Gnosis Safe to upgrade the IBCERC20 contract
- [ ] The signers have independently verified that the transaction proposal is correct
- [ ] The signers have signed the transaction proposal for the time-lock
- [ ] The facilitator has executed the transaction proposal for the time-lock
- [ ] The facilitator has executed the transaction on-chain
- [ ] The facilitator has updated the canonical deployment information for the IBCERC20 contract