package e2esuite

import (
	"testing"

	"testing"

	"github.com/stretchr/testify/require"
)

func TestGetEthAddressFromStdout(t *testing.T) {
	exampleOutput := `Compiling 1 files with Solc 0.8.25
Solc 0.8.25 finished in 2.74s
Compiler run successful!
EIP-3855 is not supported in one or more of the RPCs used.
Unsupported Chain IDs: 31337.
Contracts deployed with a Solidity version equal or higher than 0.8.20 might not work properly.
For more information, please see https://eips.ethereum.org/EIPS/eip-3855
{"logs":[],"gas_used":11048147,"returns":{"0":{"internal_type":"string","value":"\"{\\\"erc20\\\":\\\"0x51f71E738F3D4577a7d9232DAFdE6e4cAB140947\\\",\\\"ics02Client\\\":\\\"0x564dA7794e99994137470505c2A8F6Bd17002D0f\\\",\\\"ics07Tendermint\\\":\\\"0xBC05F607DDde69B2Dc571a1d565d79466a2FDD8A\\\",\\\"ics20Transfer\\\":\\\"0xb2911d67B35b582b10828465E0c76aEDbb907d75\\\",\\\"ics26Router\\\":\\\"0xbbABA12ba070C22C0c59FAdD8d8dFa3e16231aA6\\\"}\""}}}

## Setting up 1 EVM.

==========================

Chain 31337

Estimated gas price: 0.018684681 gwei

Estimated total gas used for script: 15503518

Estimated amount required: 0.000289678288207758 ETH

==========================


==========================

ONCHAIN EXECUTION COMPLETE & SUCCESSFUL.

Transactions saved to: /home/foundry./broadcast/E2ETestDeploy.s.sol/31337/run-latest.json

Sensitive values saved to: /home/foundry./cache/E2ETestDeploy.s.sol/31337/run-latest.json
`

	testSuite := &TestSuite{}
	testSuite.SetT(t)

	deployedContracts := testSuite.GetEthContractsFromDeployOutput(exampleOutput)

	require.Equal(t, "0xBC05F607DDde69B2Dc571a1d565d79466a2FDD8A", deployedContracts.Ics07Tendermint)
	require.Equal(t, "0x564dA7794e99994137470505c2A8F6Bd17002D0f", deployedContracts.Ics02Client)
	require.Equal(t, "0xbbABA12ba070C22C0c59FAdD8d8dFa3e16231aA6", deployedContracts.Ics26Router)
	require.Equal(t, "0xb2911d67B35b582b10828465E0c76aEDbb907d75", deployedContracts.Ics20Transfer)
	require.Equal(t, "0x51f71E738F3D4577a7d9232DAFdE6e4cAB140947", deployedContracts.Erc20)
}
