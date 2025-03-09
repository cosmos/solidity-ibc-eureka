## RUNBOOK - pausing transfers

### Roles

| Role        | Person |
|-------------|--------|
| Facilitator |        |
| Notekeeper  |        |
| Signers     |        |

### Runbook

1. Create an operations report (copy of this document) in docs/security/operations 

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
