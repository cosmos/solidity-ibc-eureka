alloy_sol_types::sol!("../../contracts/msgs/IICS26RouterMsgs.sol");
alloy_sol_types::sol!(
    #[sol(all_derives)]
    "../../contracts/msgs/IICS02ClientMsgs.sol"
);
alloy_sol_types::sol!(
    #[sol(all_derives)]
    "../../contracts/msgs/ILightClientMsgs.sol"
);
alloy_sol_types::sol!("../../contracts/msgs/IICS20TransferMsgs.sol");
alloy_sol_types::sol!("../../contracts/msgs/IIBCAppCallbacks.sol");

alloy_sol_types::sol!(
    #[sol(all_derives)]
    "../../contracts/light-clients/msgs/IICS07TendermintMsgs.sol"
);
alloy_sol_types::sol!("../../contracts/light-clients/msgs/ISP1Msgs.sol");
alloy_sol_types::sol!("../../contracts/light-clients/msgs/IMembershipMsgs.sol");
alloy_sol_types::sol!("../../contracts/light-clients/msgs/IMisbehaviourMsgs.sol");
alloy_sol_types::sol!("../../contracts/light-clients/msgs/IUpdateClientMsgs.sol");
alloy_sol_types::sol!("../../contracts/light-clients/msgs/IUcAndMembershipMsgs.sol");

//alloy_sol_types::sol!(
//    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
//    #[allow(missing_docs, clippy::pedantic)]
//    #[sol(type_check
//    sp1_ics07_tendermint,
//    "../../abi/SP1ICS07Tendermint.json"
//);
