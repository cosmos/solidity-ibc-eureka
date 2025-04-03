#!/bin/sh

set -euxo pipefail

mkdir -p /usr/local/relayer/sp1-programs/$SP1_PROGRAMS_VERSION
wget --no-check-certificate https://github.com/cosmos/solidity-ibc-eureka/releases/download/sp1-programs-$SP1_PROGRAMS_VERSION/sp1-ics07-tendermint-membership -O /usr/local/relayer/sp1-programs/$SP1_PROGRAMS_VERSION/sp1-ics07-tendermint-membership
wget --no-check-certificate https://github.com/cosmos/solidity-ibc-eureka/releases/download/sp1-programs-$SP1_PROGRAMS_VERSION/sp1-ics07-tendermint-update-client -O /usr/local/relayer/sp1-programs/$SP1_PROGRAMS_VERSION/sp1-ics07-tendermint-update-client
wget --no-check-certificate  https://github.com/cosmos/solidity-ibc-eureka/releases/download/sp1-programs-$SP1_PROGRAMS_VERSION/sp1-ics07-tendermint-uc-and-membership -O /usr/local/relayer/sp1-programs/$SP1_PROGRAMS_VERSION/sp1-ics07-tendermint-uc-and-membership
wget --no-check-certificate https://github.com/cosmos/solidity-ibc-eureka/releases/download/sp1-programs-$SP1_PROGRAMS_VERSION/sp1-ics07-tendermint-misbehaviour -O /usr/local/relayer/sp1-programs/$SP1_PROGRAMS_VERSION/sp1-ics07-tendermint-misbehaviour

/usr/local/bin/relayer start --config /usr/local/relayer/relayer.json
