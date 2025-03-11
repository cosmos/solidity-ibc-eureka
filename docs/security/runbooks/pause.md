## RUNBOOK - pausing transfers

### Roles

| Role        | Person |
|-------------|--------|
| Facilitator |        |
| Notekeeper  |        |

### Runbook

1. Create an operations report (copy of this document) in docs/security/operations 
2. Call the script `DEPLOYMENT_ENV=<env> FOUNDRY_ETH_RPC_URL=<rpc> forge script scripts/deployments/PauseTransfers.sol -vvvvv --broadcast --ledger`
3. Call it again without the `broadcast` flag to verify that the transfers are successfully paused

### Checklist

- [ ] The pauser has successfully broadcasted a pause transaction
- [ ] The pauser has successfully notified the council that transfers are paused and has called a security incident
- [ ] The pauser has successfully verified that transfers are paused