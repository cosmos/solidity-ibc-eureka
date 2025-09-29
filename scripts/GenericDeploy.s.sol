// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/*
    This script is used for deploying the demo contracts on testnets
*/

// solhint-disable custom-errors,gas-custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";

import { IICS02ClientMsgs } from "../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS27GMPMsgs } from "../contracts/msgs/IICS27GMPMsgs.sol";

import { ICS26Router } from "../contracts/ICS26Router.sol";
import { ICS27GMP } from "../contracts/ICS27GMP.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ICS27Lib } from "../contracts/utils/ICS27Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { DeployAccessManagerWithRoles } from "./deployments/DeployAccessManagerWithRoles.sol";
import { ICS27Account } from "../contracts/utils/ICS27Account.sol";
import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";
import { IBCXERC20 } from "../contracts/demo/IBCXERC20.sol";
import { IBCRolesLib } from "../contracts/utils/IBCRolesLib.sol";

contract DemoDeploy is Script, DeployAccessManagerWithRoles {
    using stdJson for string;

    bytes[] public cosmosMerklePrefix = [bytes("ibc"), bytes("")];

    // TODO: enter the address of your deployed light client
    address public lightClient = makeAddr("TODO");
    // TODO: enter the desired custom client id on Ethereum
    string public constant CUSTOM_CLIENT_ID = "custom-0";
    // TODO: enter the counterparty client id on the connected chain
    string public constant COUNTERPARTY_CLIENT_ID = "07-tendermint-0";

    // ERC20 parameters, adjust as needed
    string public constant ERC20_NAME = "WildFlower";
    string public constant ERC20_SYMBOL = "uwfdeposit";
    // Initial amount minted to deployer. No minting if 0.
    uint256 public constant ERC20_INITIAL_AMOUNT = 1_000_000;

    // TODO: enter the cosmos module account address that will send the bridge transfer. This can be queried with:
    // - `wfchaind q auth module-accounts`
    // - `wfchaind q auth module-account <name>`
    string public constant COSMOS_MODULE_ACCOUNT = "TODO";

    // TODO: enter the cosmos bridge account address that will receive the bridge transfer. This can be queried with:
    // - `wfchaind q gmp get-address <client-id> <ibcxerc20-address> "" `
    // NOTE:
    // - The client-id must be the one used in COUNTERPARTY_CLIENT_ID above.
    // - The ibcxerc20-address must be the checksummed address of the deployed IBCXERC20 contract on Ethereum.
    // - The last parameter is the salt, which in this case is always empty.
    // - example: `wfchaind q gmp get-address 08-wasm-1 0xEEDf36B20f5254eC49dCe502C72Bbe56335D66bd "" `
    string public constant COSMOS_BRIDGE_ACCOUNT = "TODO";

    // TODO: The directory where the contract addresses will be saved in a JSON file
    string internal constant DEPLOYMENTS_DIR = "./scripts/deployments/";

    // solhint-disable-next-line function-max-lines
    function run() public returns (string memory) {
        vm.startBroadcast();

        // ===== Step 1: Deploy IBC Eureka with proxies =====
        address ics26RouterLogic = address(new ICS26Router());
        address ics27GmpLogic = address(new ICS27GMP());
        address ibcxerc20Logic = address(new IBCXERC20());

        AccessManager accessManager = new AccessManager(msg.sender);

        ERC1967Proxy routerProxy =
            new ERC1967Proxy(ics26RouterLogic, abi.encodeCall(ICS26Router.initialize, (address(accessManager))));

        ERC1967Proxy gmpProxy = new ERC1967Proxy(
            ics27GmpLogic,
            abi.encodeCall(
                ICS27GMP.initialize, (address(routerProxy), address(new ICS27Account()), address(accessManager))
            )
        );

        // ===== Step 2: Setup access manager =====

        address ics26 = address(routerProxy);

        // Wire up the IBCAdmin and access control
        accessManager.setTargetFunctionRole(
            ics26, IBCRolesLib.ics26IdCustomizerSelectors(), IBCRolesLib.ID_CUSTOMIZER_ROLE
        );
        accessManager.setTargetFunctionRole(ics26, IBCRolesLib.ics26RelayerSelectors(), IBCRolesLib.RELAYER_ROLE);
        accessManager.setTargetFunctionRole(ics26, IBCRolesLib.uupsUpgradeSelectors(), IBCRolesLib.ADMIN_ROLE);
        // pub relay
        accessManager.setTargetFunctionRole(ics26, IBCRolesLib.ics26RelayerSelectors(), IBCRolesLib.PUBLIC_ROLE);

        // Grant roles
        accessManager.grantRole(IBCRolesLib.ID_CUSTOMIZER_ROLE, msg.sender, 0);
        accessManager.grantRole(IBCRolesLib.DELEGATE_SENDER_ROLE, msg.sender, 0);

        // ===== Step 3: Wire up the light client and GMP app =====

        // Wire GMP app
        ICS26Router(address(routerProxy)).addIBCApp(ICS27Lib.DEFAULT_PORT_ID, address(gmpProxy));
        // Wire light client
        ICS26Router(address(routerProxy)).addClient(
            CUSTOM_CLIENT_ID,
            IICS02ClientMsgs.CounterpartyInfo({ clientId: COUNTERPARTY_CLIENT_ID, merklePrefix: cosmosMerklePrefix }),
            lightClient
        );

        // ===== Step 4: Deploy and configure the demo IBCXERC20 contract =====

        // IBCXERC20
        IBCXERC20 ibcxerc20 = IBCXERC20(
            address(
                new ERC1967Proxy(
                    ibcxerc20Logic,
                    abi.encodeCall(IBCXERC20.initialize, (msg.sender, ERC20_NAME, ERC20_SYMBOL, address(gmpProxy)))
                )
            )
        );

        // Configure the IBCXERC20 contract
        ibcxerc20.setClientId(CUSTOM_CLIENT_ID);

        address bridge = ICS27GMP(address(gmpProxy)).getOrComputeAccountAddress(
            IICS27GMPMsgs.AccountIdentifier({
                clientId: CUSTOM_CLIENT_ID,
                sender: COSMOS_MODULE_ACCOUNT,
                salt: bytes("")
            })
        );
        ibcxerc20.setBridge(bridge);

        ibcxerc20.setCosmosAccount(COSMOS_BRIDGE_ACCOUNT);

        if (ERC20_INITIAL_AMOUNT > 0) {
            ibcxerc20.mint(msg.sender, ERC20_INITIAL_AMOUNT);
        }

        vm.stopBroadcast();

        string memory json = "json";
        json.serialize("ics26Router", Strings.toHexString(address(routerProxy)));
        json.serialize("ics27Gmp", Strings.toHexString(address(gmpProxy)));
        string memory finalJson = json.serialize("ibcxerc20", Strings.toHexString(address(ibcxerc20)));

        string memory fileName = string.concat(DEPLOYMENTS_DIR, "deployment.json");
        vm.writeFile(fileName, finalJson);

        return finalJson;
    }
}
