use tendermint_light_client_verifier::types::TrustThreshold as TendermintTrustThreshold;

#[cfg(feature = "rpc")]
alloy_sol_types::sol!(
    #[sol(rpc)]
    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
    #[allow(missing_docs, clippy::pedantic, warnings)]
    sp1_ics07_tendermint,
    "../../abi/SP1ICS07Tendermint.json"
);

// NOTE: The riscv program won't compile with the `rpc` features.
#[cfg(not(feature = "rpc"))]
alloy_sol_types::sol!(
    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
    #[allow(missing_docs, clippy::pedantic)]
    sp1_ics07_tendermint,
    "../../abi/SP1ICS07Tendermint.json"
);

#[allow(clippy::fallible_impl_from)]
impl From<IICS07TendermintMsgs::TrustThreshold> for TendermintTrustThreshold {
    fn from(trust_threshold: IICS07TendermintMsgs::TrustThreshold) -> Self {
        Self::new(
            trust_threshold.numerator.into(),
            trust_threshold.denominator.into(),
        )
        .unwrap()
    }
}

impl TryFrom<TendermintTrustThreshold> for IICS07TendermintMsgs::TrustThreshold {
    type Error = <u64 as TryInto<u32>>::Error;

    fn try_from(trust_threshold: TendermintTrustThreshold) -> Result<Self, Self::Error> {
        Ok(Self {
            numerator: trust_threshold.numerator().try_into()?,
            denominator: trust_threshold.denominator().try_into()?,
        })
    }
}

impl TryFrom<ibc_core_client_types::Height> for IICS02ClientMsgs::Height {
    type Error = <u64 as TryInto<u32>>::Error;

    fn try_from(height: ibc_core_client_types::Height) -> Result<Self, Self::Error> {
        Ok(Self {
            revisionNumber: height.revision_number().try_into()?,
            revisionHeight: height.revision_height().try_into()?,
        })
    }
}
