// Code generated - DO NOT EDIT.
// This file is a generated binding and any manual changes will be lost.

package sp1ics07tendermint

import (
	"errors"
	"math/big"
	"strings"

	ethereum "github.com/ethereum/go-ethereum"
	"github.com/ethereum/go-ethereum/accounts/abi"
	"github.com/ethereum/go-ethereum/accounts/abi/bind"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/event"
)

// Reference imports to suppress errors if they are not otherwise used.
var (
	_ = errors.New
	_ = big.NewInt
	_ = strings.NewReader
	_ = ethereum.NotFound
	_ = bind.Bind
	_ = common.Big1
	_ = types.BloomLookup
	_ = event.NewSubscription
	_ = abi.ConvertType
)

// IICS02ClientMsgsHeight is an auto generated low-level Go binding around an user-defined struct.
type IICS02ClientMsgsHeight struct {
	RevisionNumber uint32
	RevisionHeight uint32
}

// IICS07TendermintMsgsClientState is an auto generated low-level Go binding around an user-defined struct.
type IICS07TendermintMsgsClientState struct {
	ChainId         string
	TrustLevel      IICS07TendermintMsgsTrustThreshold
	LatestHeight    IICS02ClientMsgsHeight
	TrustingPeriod  uint32
	UnbondingPeriod uint32
	IsFrozen        bool
}

// IICS07TendermintMsgsConsensusState is an auto generated low-level Go binding around an user-defined struct.
type IICS07TendermintMsgsConsensusState struct {
	Timestamp          uint64
	Root               [32]byte
	NextValidatorsHash [32]byte
}

// IICS07TendermintMsgsTrustThreshold is an auto generated low-level Go binding around an user-defined struct.
type IICS07TendermintMsgsTrustThreshold struct {
	Numerator   uint8
	Denominator uint8
}

// ILightClientMsgsMsgMembership is an auto generated low-level Go binding around an user-defined struct.
type ILightClientMsgsMsgMembership struct {
	Proof       []byte
	ProofHeight IICS02ClientMsgsHeight
	Path        [][]byte
	Value       []byte
}

// IMembershipMsgsKVPair is an auto generated low-level Go binding around an user-defined struct.
type IMembershipMsgsKVPair struct {
	Path  [][]byte
	Value []byte
}

// IMembershipMsgsMembershipOutput is an auto generated low-level Go binding around an user-defined struct.
type IMembershipMsgsMembershipOutput struct {
	CommitmentRoot [32]byte
	KvPairs        []IMembershipMsgsKVPair
}

// IMembershipMsgsMembershipProof is an auto generated low-level Go binding around an user-defined struct.
type IMembershipMsgsMembershipProof struct {
	ProofType uint8
	Proof     []byte
}

// IMembershipMsgsSP1MembershipAndUpdateClientProof is an auto generated low-level Go binding around an user-defined struct.
type IMembershipMsgsSP1MembershipAndUpdateClientProof struct {
	Sp1Proof ISP1MsgsSP1Proof
}

// IMembershipMsgsSP1MembershipProof is an auto generated low-level Go binding around an user-defined struct.
type IMembershipMsgsSP1MembershipProof struct {
	Sp1Proof              ISP1MsgsSP1Proof
	TrustedConsensusState IICS07TendermintMsgsConsensusState
}

// ISP1MsgsSP1Proof is an auto generated low-level Go binding around an user-defined struct.
type ISP1MsgsSP1Proof struct {
	VKey         [32]byte
	PublicValues []byte
	Proof        []byte
}

// IUpdateClientAndMembershipMsgsUcAndMembershipOutput is an auto generated low-level Go binding around an user-defined struct.
type IUpdateClientAndMembershipMsgsUcAndMembershipOutput struct {
	UpdateClientOutput IUpdateClientMsgsUpdateClientOutput
	KvPairs            []IMembershipMsgsKVPair
}

// IUpdateClientMsgsEnv is an auto generated low-level Go binding around an user-defined struct.
type IUpdateClientMsgsEnv struct {
	ChainId        string
	TrustThreshold IICS07TendermintMsgsTrustThreshold
	TrustingPeriod uint32
	Now            uint64
}

// IUpdateClientMsgsMsgUpdateClient is an auto generated low-level Go binding around an user-defined struct.
type IUpdateClientMsgsMsgUpdateClient struct {
	Sp1Proof ISP1MsgsSP1Proof
}

// IUpdateClientMsgsUpdateClientOutput is an auto generated low-level Go binding around an user-defined struct.
type IUpdateClientMsgsUpdateClientOutput struct {
	TrustedConsensusState IICS07TendermintMsgsConsensusState
	NewConsensusState     IICS07TendermintMsgsConsensusState
	Env                   IUpdateClientMsgsEnv
	TrustedHeight         IICS02ClientMsgsHeight
	NewHeight             IICS02ClientMsgsHeight
}

// ContractMetaData contains all meta data concerning the Contract contract.
var ContractMetaData = &bind.MetaData{
	ABI: "[{\"type\":\"constructor\",\"inputs\":[{\"name\":\"updateClientProgramVkey\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"membershipProgramVkey\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"updateClientAndMembershipProgramVkey\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"verifier\",\"type\":\"address\",\"internalType\":\"address\"},{\"name\":\"_clientState\",\"type\":\"bytes\",\"internalType\":\"bytes\"},{\"name\":\"_consensusState\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}],\"stateMutability\":\"nonpayable\"},{\"type\":\"function\",\"name\":\"ALLOWED_SP1_CLOCK_DRIFT\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"uint16\",\"internalType\":\"uint16\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"MEMBERSHIP_PROGRAM_VKEY\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"UPDATE_CLIENT_AND_MEMBERSHIP_PROGRAM_VKEY\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"UPDATE_CLIENT_PROGRAM_VKEY\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"VERIFIER\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"address\",\"internalType\":\"contractISP1Verifier\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"abiPublicTypes\",\"inputs\":[{\"name\":\"o1\",\"type\":\"tuple\",\"internalType\":\"structIMembershipMsgs.MembershipOutput\",\"components\":[{\"name\":\"commitmentRoot\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"kvPairs\",\"type\":\"tuple[]\",\"internalType\":\"structIMembershipMsgs.KVPair[]\",\"components\":[{\"name\":\"path\",\"type\":\"bytes[]\",\"internalType\":\"bytes[]\"},{\"name\":\"value\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]}]},{\"name\":\"o2\",\"type\":\"tuple\",\"internalType\":\"structIUpdateClientAndMembershipMsgs.UcAndMembershipOutput\",\"components\":[{\"name\":\"updateClientOutput\",\"type\":\"tuple\",\"internalType\":\"structIUpdateClientMsgs.UpdateClientOutput\",\"components\":[{\"name\":\"trustedConsensusState\",\"type\":\"tuple\",\"internalType\":\"structIICS07TendermintMsgs.ConsensusState\",\"components\":[{\"name\":\"timestamp\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"root\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"nextValidatorsHash\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}]},{\"name\":\"newConsensusState\",\"type\":\"tuple\",\"internalType\":\"structIICS07TendermintMsgs.ConsensusState\",\"components\":[{\"name\":\"timestamp\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"root\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"nextValidatorsHash\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}]},{\"name\":\"env\",\"type\":\"tuple\",\"internalType\":\"structIUpdateClientMsgs.Env\",\"components\":[{\"name\":\"chainId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"trustThreshold\",\"type\":\"tuple\",\"internalType\":\"structIICS07TendermintMsgs.TrustThreshold\",\"components\":[{\"name\":\"numerator\",\"type\":\"uint8\",\"internalType\":\"uint8\"},{\"name\":\"denominator\",\"type\":\"uint8\",\"internalType\":\"uint8\"}]},{\"name\":\"trustingPeriod\",\"type\":\"uint32\",\"internalType\":\"uint32\"},{\"name\":\"now\",\"type\":\"uint64\",\"internalType\":\"uint64\"}]},{\"name\":\"trustedHeight\",\"type\":\"tuple\",\"internalType\":\"structIICS02ClientMsgs.Height\",\"components\":[{\"name\":\"revisionNumber\",\"type\":\"uint32\",\"internalType\":\"uint32\"},{\"name\":\"revisionHeight\",\"type\":\"uint32\",\"internalType\":\"uint32\"}]},{\"name\":\"newHeight\",\"type\":\"tuple\",\"internalType\":\"structIICS02ClientMsgs.Height\",\"components\":[{\"name\":\"revisionNumber\",\"type\":\"uint32\",\"internalType\":\"uint32\"},{\"name\":\"revisionHeight\",\"type\":\"uint32\",\"internalType\":\"uint32\"}]}]},{\"name\":\"kvPairs\",\"type\":\"tuple[]\",\"internalType\":\"structIMembershipMsgs.KVPair[]\",\"components\":[{\"name\":\"path\",\"type\":\"bytes[]\",\"internalType\":\"bytes[]\"},{\"name\":\"value\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]}]},{\"name\":\"o3\",\"type\":\"tuple\",\"internalType\":\"structIUpdateClientMsgs.MsgUpdateClient\",\"components\":[{\"name\":\"sp1Proof\",\"type\":\"tuple\",\"internalType\":\"structISP1Msgs.SP1Proof\",\"components\":[{\"name\":\"vKey\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"publicValues\",\"type\":\"bytes\",\"internalType\":\"bytes\"},{\"name\":\"proof\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]}]},{\"name\":\"o4\",\"type\":\"tuple\",\"internalType\":\"structIMembershipMsgs.MembershipProof\",\"components\":[{\"name\":\"proofType\",\"type\":\"uint8\",\"internalType\":\"enumIMembershipMsgs.MembershipProofType\"},{\"name\":\"proof\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]},{\"name\":\"o5\",\"type\":\"tuple\",\"internalType\":\"structIMembershipMsgs.SP1MembershipProof\",\"components\":[{\"name\":\"sp1Proof\",\"type\":\"tuple\",\"internalType\":\"structISP1Msgs.SP1Proof\",\"components\":[{\"name\":\"vKey\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"publicValues\",\"type\":\"bytes\",\"internalType\":\"bytes\"},{\"name\":\"proof\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]},{\"name\":\"trustedConsensusState\",\"type\":\"tuple\",\"internalType\":\"structIICS07TendermintMsgs.ConsensusState\",\"components\":[{\"name\":\"timestamp\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"root\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"nextValidatorsHash\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}]}]},{\"name\":\"o6\",\"type\":\"tuple\",\"internalType\":\"structIMembershipMsgs.SP1MembershipAndUpdateClientProof\",\"components\":[{\"name\":\"sp1Proof\",\"type\":\"tuple\",\"internalType\":\"structISP1Msgs.SP1Proof\",\"components\":[{\"name\":\"vKey\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"publicValues\",\"type\":\"bytes\",\"internalType\":\"bytes\"},{\"name\":\"proof\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]}]}],\"outputs\":[],\"stateMutability\":\"pure\"},{\"type\":\"function\",\"name\":\"getClientState\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"tuple\",\"internalType\":\"structIICS07TendermintMsgs.ClientState\",\"components\":[{\"name\":\"chainId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"trustLevel\",\"type\":\"tuple\",\"internalType\":\"structIICS07TendermintMsgs.TrustThreshold\",\"components\":[{\"name\":\"numerator\",\"type\":\"uint8\",\"internalType\":\"uint8\"},{\"name\":\"denominator\",\"type\":\"uint8\",\"internalType\":\"uint8\"}]},{\"name\":\"latestHeight\",\"type\":\"tuple\",\"internalType\":\"structIICS02ClientMsgs.Height\",\"components\":[{\"name\":\"revisionNumber\",\"type\":\"uint32\",\"internalType\":\"uint32\"},{\"name\":\"revisionHeight\",\"type\":\"uint32\",\"internalType\":\"uint32\"}]},{\"name\":\"trustingPeriod\",\"type\":\"uint32\",\"internalType\":\"uint32\"},{\"name\":\"unbondingPeriod\",\"type\":\"uint32\",\"internalType\":\"uint32\"},{\"name\":\"isFrozen\",\"type\":\"bool\",\"internalType\":\"bool\"}]}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"getConsensusStateHash\",\"inputs\":[{\"name\":\"revisionHeight\",\"type\":\"uint32\",\"internalType\":\"uint32\"}],\"outputs\":[{\"name\":\"\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"membership\",\"inputs\":[{\"name\":\"msgMembership\",\"type\":\"tuple\",\"internalType\":\"structILightClientMsgs.MsgMembership\",\"components\":[{\"name\":\"proof\",\"type\":\"bytes\",\"internalType\":\"bytes\"},{\"name\":\"proofHeight\",\"type\":\"tuple\",\"internalType\":\"structIICS02ClientMsgs.Height\",\"components\":[{\"name\":\"revisionNumber\",\"type\":\"uint32\",\"internalType\":\"uint32\"},{\"name\":\"revisionHeight\",\"type\":\"uint32\",\"internalType\":\"uint32\"}]},{\"name\":\"path\",\"type\":\"bytes[]\",\"internalType\":\"bytes[]\"},{\"name\":\"value\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]}],\"outputs\":[{\"name\":\"timestamp\",\"type\":\"uint256\",\"internalType\":\"uint256\"}],\"stateMutability\":\"nonpayable\"},{\"type\":\"function\",\"name\":\"misbehaviour\",\"inputs\":[{\"name\":\"\",\"type\":\"bytes\",\"internalType\":\"bytes\"}],\"outputs\":[],\"stateMutability\":\"pure\"},{\"type\":\"function\",\"name\":\"updateClient\",\"inputs\":[{\"name\":\"updateMsg\",\"type\":\"bytes\",\"internalType\":\"bytes\"}],\"outputs\":[{\"name\":\"\",\"type\":\"uint8\",\"internalType\":\"enumILightClientMsgs.UpdateResult\"}],\"stateMutability\":\"nonpayable\"},{\"type\":\"function\",\"name\":\"upgradeClient\",\"inputs\":[{\"name\":\"\",\"type\":\"bytes\",\"internalType\":\"bytes\"}],\"outputs\":[],\"stateMutability\":\"pure\"},{\"type\":\"error\",\"name\":\"CannotHandleMisbehavior\",\"inputs\":[]},{\"type\":\"error\",\"name\":\"ChainIdMismatch\",\"inputs\":[{\"name\":\"expected\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"actual\",\"type\":\"string\",\"internalType\":\"string\"}]},{\"type\":\"error\",\"name\":\"ConsensusStateHashMismatch\",\"inputs\":[{\"name\":\"expected\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"actual\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}]},{\"type\":\"error\",\"name\":\"ConsensusStateNotFound\",\"inputs\":[]},{\"type\":\"error\",\"name\":\"ConsensusStateRootMismatch\",\"inputs\":[{\"name\":\"expected\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"actual\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}]},{\"type\":\"error\",\"name\":\"FeatureNotSupported\",\"inputs\":[]},{\"type\":\"error\",\"name\":\"FrozenClientState\",\"inputs\":[]},{\"type\":\"error\",\"name\":\"LengthIsOutOfRange\",\"inputs\":[{\"name\":\"length\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"min\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"max\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"type\":\"error\",\"name\":\"MembershipProofKeyNotFound\",\"inputs\":[{\"name\":\"path\",\"type\":\"bytes[]\",\"internalType\":\"bytes[]\"}]},{\"type\":\"error\",\"name\":\"MembershipProofValueMismatch\",\"inputs\":[{\"name\":\"expected\",\"type\":\"bytes\",\"internalType\":\"bytes\"},{\"name\":\"actual\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]},{\"type\":\"error\",\"name\":\"ProofHeightMismatch\",\"inputs\":[{\"name\":\"expectedRevisionNumber\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"expectedRevisionHeight\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"actualRevisionNumber\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"actualRevisionHeight\",\"type\":\"uint64\",\"internalType\":\"uint64\"}]},{\"type\":\"error\",\"name\":\"ProofIsInTheFuture\",\"inputs\":[{\"name\":\"now\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"proofTimestamp\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"type\":\"error\",\"name\":\"ProofIsTooOld\",\"inputs\":[{\"name\":\"now\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"proofTimestamp\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"type\":\"error\",\"name\":\"TrustThresholdMismatch\",\"inputs\":[{\"name\":\"expectedNumerator\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"expectedDenominator\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"actualNumerator\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"actualDenominator\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"type\":\"error\",\"name\":\"TrustingPeriodMismatch\",\"inputs\":[{\"name\":\"expected\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"actual\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"type\":\"error\",\"name\":\"TrustingPeriodTooLong\",\"inputs\":[{\"name\":\"trustingPeriod\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"unbondingPeriod\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"type\":\"error\",\"name\":\"UnknownMembershipProofType\",\"inputs\":[{\"name\":\"proofType\",\"type\":\"uint8\",\"internalType\":\"uint8\"}]},{\"type\":\"error\",\"name\":\"VerificationKeyMismatch\",\"inputs\":[{\"name\":\"expected\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"},{\"name\":\"actual\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}]}]",
}

// ContractABI is the input ABI used to generate the binding from.
// Deprecated: Use ContractMetaData.ABI instead.
var ContractABI = ContractMetaData.ABI

// Contract is an auto generated Go binding around an Ethereum contract.
type Contract struct {
	ContractCaller     // Read-only binding to the contract
	ContractTransactor // Write-only binding to the contract
	ContractFilterer   // Log filterer for contract events
}

// ContractCaller is an auto generated read-only Go binding around an Ethereum contract.
type ContractCaller struct {
	contract *bind.BoundContract // Generic contract wrapper for the low level calls
}

// ContractTransactor is an auto generated write-only Go binding around an Ethereum contract.
type ContractTransactor struct {
	contract *bind.BoundContract // Generic contract wrapper for the low level calls
}

// ContractFilterer is an auto generated log filtering Go binding around an Ethereum contract events.
type ContractFilterer struct {
	contract *bind.BoundContract // Generic contract wrapper for the low level calls
}

// ContractSession is an auto generated Go binding around an Ethereum contract,
// with pre-set call and transact options.
type ContractSession struct {
	Contract     *Contract         // Generic contract binding to set the session for
	CallOpts     bind.CallOpts     // Call options to use throughout this session
	TransactOpts bind.TransactOpts // Transaction auth options to use throughout this session
}

// ContractCallerSession is an auto generated read-only Go binding around an Ethereum contract,
// with pre-set call options.
type ContractCallerSession struct {
	Contract *ContractCaller // Generic contract caller binding to set the session for
	CallOpts bind.CallOpts   // Call options to use throughout this session
}

// ContractTransactorSession is an auto generated write-only Go binding around an Ethereum contract,
// with pre-set transact options.
type ContractTransactorSession struct {
	Contract     *ContractTransactor // Generic contract transactor binding to set the session for
	TransactOpts bind.TransactOpts   // Transaction auth options to use throughout this session
}

// ContractRaw is an auto generated low-level Go binding around an Ethereum contract.
type ContractRaw struct {
	Contract *Contract // Generic contract binding to access the raw methods on
}

// ContractCallerRaw is an auto generated low-level read-only Go binding around an Ethereum contract.
type ContractCallerRaw struct {
	Contract *ContractCaller // Generic read-only contract binding to access the raw methods on
}

// ContractTransactorRaw is an auto generated low-level write-only Go binding around an Ethereum contract.
type ContractTransactorRaw struct {
	Contract *ContractTransactor // Generic write-only contract binding to access the raw methods on
}

// NewContract creates a new instance of Contract, bound to a specific deployed contract.
func NewContract(address common.Address, backend bind.ContractBackend) (*Contract, error) {
	contract, err := bindContract(address, backend, backend, backend)
	if err != nil {
		return nil, err
	}
	return &Contract{ContractCaller: ContractCaller{contract: contract}, ContractTransactor: ContractTransactor{contract: contract}, ContractFilterer: ContractFilterer{contract: contract}}, nil
}

// NewContractCaller creates a new read-only instance of Contract, bound to a specific deployed contract.
func NewContractCaller(address common.Address, caller bind.ContractCaller) (*ContractCaller, error) {
	contract, err := bindContract(address, caller, nil, nil)
	if err != nil {
		return nil, err
	}
	return &ContractCaller{contract: contract}, nil
}

// NewContractTransactor creates a new write-only instance of Contract, bound to a specific deployed contract.
func NewContractTransactor(address common.Address, transactor bind.ContractTransactor) (*ContractTransactor, error) {
	contract, err := bindContract(address, nil, transactor, nil)
	if err != nil {
		return nil, err
	}
	return &ContractTransactor{contract: contract}, nil
}

// NewContractFilterer creates a new log filterer instance of Contract, bound to a specific deployed contract.
func NewContractFilterer(address common.Address, filterer bind.ContractFilterer) (*ContractFilterer, error) {
	contract, err := bindContract(address, nil, nil, filterer)
	if err != nil {
		return nil, err
	}
	return &ContractFilterer{contract: contract}, nil
}

// bindContract binds a generic wrapper to an already deployed contract.
func bindContract(address common.Address, caller bind.ContractCaller, transactor bind.ContractTransactor, filterer bind.ContractFilterer) (*bind.BoundContract, error) {
	parsed, err := ContractMetaData.GetAbi()
	if err != nil {
		return nil, err
	}
	return bind.NewBoundContract(address, *parsed, caller, transactor, filterer), nil
}

// Call invokes the (constant) contract method with params as input values and
// sets the output to result. The result type might be a single field for simple
// returns, a slice of interfaces for anonymous returns and a struct for named
// returns.
func (_Contract *ContractRaw) Call(opts *bind.CallOpts, result *[]interface{}, method string, params ...interface{}) error {
	return _Contract.Contract.ContractCaller.contract.Call(opts, result, method, params...)
}

// Transfer initiates a plain transaction to move funds to the contract, calling
// its default method if one is available.
func (_Contract *ContractRaw) Transfer(opts *bind.TransactOpts) (*types.Transaction, error) {
	return _Contract.Contract.ContractTransactor.contract.Transfer(opts)
}

// Transact invokes the (paid) contract method with params as input values.
func (_Contract *ContractRaw) Transact(opts *bind.TransactOpts, method string, params ...interface{}) (*types.Transaction, error) {
	return _Contract.Contract.ContractTransactor.contract.Transact(opts, method, params...)
}

// Call invokes the (constant) contract method with params as input values and
// sets the output to result. The result type might be a single field for simple
// returns, a slice of interfaces for anonymous returns and a struct for named
// returns.
func (_Contract *ContractCallerRaw) Call(opts *bind.CallOpts, result *[]interface{}, method string, params ...interface{}) error {
	return _Contract.Contract.contract.Call(opts, result, method, params...)
}

// Transfer initiates a plain transaction to move funds to the contract, calling
// its default method if one is available.
func (_Contract *ContractTransactorRaw) Transfer(opts *bind.TransactOpts) (*types.Transaction, error) {
	return _Contract.Contract.contract.Transfer(opts)
}

// Transact invokes the (paid) contract method with params as input values.
func (_Contract *ContractTransactorRaw) Transact(opts *bind.TransactOpts, method string, params ...interface{}) (*types.Transaction, error) {
	return _Contract.Contract.contract.Transact(opts, method, params...)
}

// ALLOWEDSP1CLOCKDRIFT is a free data retrieval call binding the contract method 0x2c3ee474.
//
// Solidity: function ALLOWED_SP1_CLOCK_DRIFT() view returns(uint16)
func (_Contract *ContractCaller) ALLOWEDSP1CLOCKDRIFT(opts *bind.CallOpts) (uint16, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "ALLOWED_SP1_CLOCK_DRIFT")

	if err != nil {
		return *new(uint16), err
	}

	out0 := *abi.ConvertType(out[0], new(uint16)).(*uint16)

	return out0, err

}

// ALLOWEDSP1CLOCKDRIFT is a free data retrieval call binding the contract method 0x2c3ee474.
//
// Solidity: function ALLOWED_SP1_CLOCK_DRIFT() view returns(uint16)
func (_Contract *ContractSession) ALLOWEDSP1CLOCKDRIFT() (uint16, error) {
	return _Contract.Contract.ALLOWEDSP1CLOCKDRIFT(&_Contract.CallOpts)
}

// ALLOWEDSP1CLOCKDRIFT is a free data retrieval call binding the contract method 0x2c3ee474.
//
// Solidity: function ALLOWED_SP1_CLOCK_DRIFT() view returns(uint16)
func (_Contract *ContractCallerSession) ALLOWEDSP1CLOCKDRIFT() (uint16, error) {
	return _Contract.Contract.ALLOWEDSP1CLOCKDRIFT(&_Contract.CallOpts)
}

// MEMBERSHIPPROGRAMVKEY is a free data retrieval call binding the contract method 0xe45a6d0d.
//
// Solidity: function MEMBERSHIP_PROGRAM_VKEY() view returns(bytes32)
func (_Contract *ContractCaller) MEMBERSHIPPROGRAMVKEY(opts *bind.CallOpts) ([32]byte, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "MEMBERSHIP_PROGRAM_VKEY")

	if err != nil {
		return *new([32]byte), err
	}

	out0 := *abi.ConvertType(out[0], new([32]byte)).(*[32]byte)

	return out0, err

}

// MEMBERSHIPPROGRAMVKEY is a free data retrieval call binding the contract method 0xe45a6d0d.
//
// Solidity: function MEMBERSHIP_PROGRAM_VKEY() view returns(bytes32)
func (_Contract *ContractSession) MEMBERSHIPPROGRAMVKEY() ([32]byte, error) {
	return _Contract.Contract.MEMBERSHIPPROGRAMVKEY(&_Contract.CallOpts)
}

// MEMBERSHIPPROGRAMVKEY is a free data retrieval call binding the contract method 0xe45a6d0d.
//
// Solidity: function MEMBERSHIP_PROGRAM_VKEY() view returns(bytes32)
func (_Contract *ContractCallerSession) MEMBERSHIPPROGRAMVKEY() ([32]byte, error) {
	return _Contract.Contract.MEMBERSHIPPROGRAMVKEY(&_Contract.CallOpts)
}

// UPDATECLIENTANDMEMBERSHIPPROGRAMVKEY is a free data retrieval call binding the contract method 0x0225293e.
//
// Solidity: function UPDATE_CLIENT_AND_MEMBERSHIP_PROGRAM_VKEY() view returns(bytes32)
func (_Contract *ContractCaller) UPDATECLIENTANDMEMBERSHIPPROGRAMVKEY(opts *bind.CallOpts) ([32]byte, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "UPDATE_CLIENT_AND_MEMBERSHIP_PROGRAM_VKEY")

	if err != nil {
		return *new([32]byte), err
	}

	out0 := *abi.ConvertType(out[0], new([32]byte)).(*[32]byte)

	return out0, err

}

// UPDATECLIENTANDMEMBERSHIPPROGRAMVKEY is a free data retrieval call binding the contract method 0x0225293e.
//
// Solidity: function UPDATE_CLIENT_AND_MEMBERSHIP_PROGRAM_VKEY() view returns(bytes32)
func (_Contract *ContractSession) UPDATECLIENTANDMEMBERSHIPPROGRAMVKEY() ([32]byte, error) {
	return _Contract.Contract.UPDATECLIENTANDMEMBERSHIPPROGRAMVKEY(&_Contract.CallOpts)
}

// UPDATECLIENTANDMEMBERSHIPPROGRAMVKEY is a free data retrieval call binding the contract method 0x0225293e.
//
// Solidity: function UPDATE_CLIENT_AND_MEMBERSHIP_PROGRAM_VKEY() view returns(bytes32)
func (_Contract *ContractCallerSession) UPDATECLIENTANDMEMBERSHIPPROGRAMVKEY() ([32]byte, error) {
	return _Contract.Contract.UPDATECLIENTANDMEMBERSHIPPROGRAMVKEY(&_Contract.CallOpts)
}

// UPDATECLIENTPROGRAMVKEY is a free data retrieval call binding the contract method 0xca7242f9.
//
// Solidity: function UPDATE_CLIENT_PROGRAM_VKEY() view returns(bytes32)
func (_Contract *ContractCaller) UPDATECLIENTPROGRAMVKEY(opts *bind.CallOpts) ([32]byte, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "UPDATE_CLIENT_PROGRAM_VKEY")

	if err != nil {
		return *new([32]byte), err
	}

	out0 := *abi.ConvertType(out[0], new([32]byte)).(*[32]byte)

	return out0, err

}

// UPDATECLIENTPROGRAMVKEY is a free data retrieval call binding the contract method 0xca7242f9.
//
// Solidity: function UPDATE_CLIENT_PROGRAM_VKEY() view returns(bytes32)
func (_Contract *ContractSession) UPDATECLIENTPROGRAMVKEY() ([32]byte, error) {
	return _Contract.Contract.UPDATECLIENTPROGRAMVKEY(&_Contract.CallOpts)
}

// UPDATECLIENTPROGRAMVKEY is a free data retrieval call binding the contract method 0xca7242f9.
//
// Solidity: function UPDATE_CLIENT_PROGRAM_VKEY() view returns(bytes32)
func (_Contract *ContractCallerSession) UPDATECLIENTPROGRAMVKEY() ([32]byte, error) {
	return _Contract.Contract.UPDATECLIENTPROGRAMVKEY(&_Contract.CallOpts)
}

// VERIFIER is a free data retrieval call binding the contract method 0x08c84e70.
//
// Solidity: function VERIFIER() view returns(address)
func (_Contract *ContractCaller) VERIFIER(opts *bind.CallOpts) (common.Address, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "VERIFIER")

	if err != nil {
		return *new(common.Address), err
	}

	out0 := *abi.ConvertType(out[0], new(common.Address)).(*common.Address)

	return out0, err

}

// VERIFIER is a free data retrieval call binding the contract method 0x08c84e70.
//
// Solidity: function VERIFIER() view returns(address)
func (_Contract *ContractSession) VERIFIER() (common.Address, error) {
	return _Contract.Contract.VERIFIER(&_Contract.CallOpts)
}

// VERIFIER is a free data retrieval call binding the contract method 0x08c84e70.
//
// Solidity: function VERIFIER() view returns(address)
func (_Contract *ContractCallerSession) VERIFIER() (common.Address, error) {
	return _Contract.Contract.VERIFIER(&_Contract.CallOpts)
}

// AbiPublicTypes is a free data retrieval call binding the contract method 0x342467e1.
//
// Solidity: function abiPublicTypes((bytes32,(bytes[],bytes)[]) o1, (((uint64,bytes32,bytes32),(uint64,bytes32,bytes32),(string,(uint8,uint8),uint32,uint64),(uint32,uint32),(uint32,uint32)),(bytes[],bytes)[]) o2, ((bytes32,bytes,bytes)) o3, (uint8,bytes) o4, ((bytes32,bytes,bytes),(uint64,bytes32,bytes32)) o5, ((bytes32,bytes,bytes)) o6) pure returns()
func (_Contract *ContractCaller) AbiPublicTypes(opts *bind.CallOpts, o1 IMembershipMsgsMembershipOutput, o2 IUpdateClientAndMembershipMsgsUcAndMembershipOutput, o3 IUpdateClientMsgsMsgUpdateClient, o4 IMembershipMsgsMembershipProof, o5 IMembershipMsgsSP1MembershipProof, o6 IMembershipMsgsSP1MembershipAndUpdateClientProof) error {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "abiPublicTypes", o1, o2, o3, o4, o5, o6)

	if err != nil {
		return err
	}

	return err

}

// AbiPublicTypes is a free data retrieval call binding the contract method 0x342467e1.
//
// Solidity: function abiPublicTypes((bytes32,(bytes[],bytes)[]) o1, (((uint64,bytes32,bytes32),(uint64,bytes32,bytes32),(string,(uint8,uint8),uint32,uint64),(uint32,uint32),(uint32,uint32)),(bytes[],bytes)[]) o2, ((bytes32,bytes,bytes)) o3, (uint8,bytes) o4, ((bytes32,bytes,bytes),(uint64,bytes32,bytes32)) o5, ((bytes32,bytes,bytes)) o6) pure returns()
func (_Contract *ContractSession) AbiPublicTypes(o1 IMembershipMsgsMembershipOutput, o2 IUpdateClientAndMembershipMsgsUcAndMembershipOutput, o3 IUpdateClientMsgsMsgUpdateClient, o4 IMembershipMsgsMembershipProof, o5 IMembershipMsgsSP1MembershipProof, o6 IMembershipMsgsSP1MembershipAndUpdateClientProof) error {
	return _Contract.Contract.AbiPublicTypes(&_Contract.CallOpts, o1, o2, o3, o4, o5, o6)
}

// AbiPublicTypes is a free data retrieval call binding the contract method 0x342467e1.
//
// Solidity: function abiPublicTypes((bytes32,(bytes[],bytes)[]) o1, (((uint64,bytes32,bytes32),(uint64,bytes32,bytes32),(string,(uint8,uint8),uint32,uint64),(uint32,uint32),(uint32,uint32)),(bytes[],bytes)[]) o2, ((bytes32,bytes,bytes)) o3, (uint8,bytes) o4, ((bytes32,bytes,bytes),(uint64,bytes32,bytes32)) o5, ((bytes32,bytes,bytes)) o6) pure returns()
func (_Contract *ContractCallerSession) AbiPublicTypes(o1 IMembershipMsgsMembershipOutput, o2 IUpdateClientAndMembershipMsgsUcAndMembershipOutput, o3 IUpdateClientMsgsMsgUpdateClient, o4 IMembershipMsgsMembershipProof, o5 IMembershipMsgsSP1MembershipProof, o6 IMembershipMsgsSP1MembershipAndUpdateClientProof) error {
	return _Contract.Contract.AbiPublicTypes(&_Contract.CallOpts, o1, o2, o3, o4, o5, o6)
}

// GetClientState is a free data retrieval call binding the contract method 0xef913a4b.
//
// Solidity: function getClientState() view returns((string,(uint8,uint8),(uint32,uint32),uint32,uint32,bool))
func (_Contract *ContractCaller) GetClientState(opts *bind.CallOpts) (IICS07TendermintMsgsClientState, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "getClientState")

	if err != nil {
		return *new(IICS07TendermintMsgsClientState), err
	}

	out0 := *abi.ConvertType(out[0], new(IICS07TendermintMsgsClientState)).(*IICS07TendermintMsgsClientState)

	return out0, err

}

// GetClientState is a free data retrieval call binding the contract method 0xef913a4b.
//
// Solidity: function getClientState() view returns((string,(uint8,uint8),(uint32,uint32),uint32,uint32,bool))
func (_Contract *ContractSession) GetClientState() (IICS07TendermintMsgsClientState, error) {
	return _Contract.Contract.GetClientState(&_Contract.CallOpts)
}

// GetClientState is a free data retrieval call binding the contract method 0xef913a4b.
//
// Solidity: function getClientState() view returns((string,(uint8,uint8),(uint32,uint32),uint32,uint32,bool))
func (_Contract *ContractCallerSession) GetClientState() (IICS07TendermintMsgsClientState, error) {
	return _Contract.Contract.GetClientState(&_Contract.CallOpts)
}

// GetConsensusStateHash is a free data retrieval call binding the contract method 0x0e954111.
//
// Solidity: function getConsensusStateHash(uint32 revisionHeight) view returns(bytes32)
func (_Contract *ContractCaller) GetConsensusStateHash(opts *bind.CallOpts, revisionHeight uint32) ([32]byte, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "getConsensusStateHash", revisionHeight)

	if err != nil {
		return *new([32]byte), err
	}

	out0 := *abi.ConvertType(out[0], new([32]byte)).(*[32]byte)

	return out0, err

}

// GetConsensusStateHash is a free data retrieval call binding the contract method 0x0e954111.
//
// Solidity: function getConsensusStateHash(uint32 revisionHeight) view returns(bytes32)
func (_Contract *ContractSession) GetConsensusStateHash(revisionHeight uint32) ([32]byte, error) {
	return _Contract.Contract.GetConsensusStateHash(&_Contract.CallOpts, revisionHeight)
}

// GetConsensusStateHash is a free data retrieval call binding the contract method 0x0e954111.
//
// Solidity: function getConsensusStateHash(uint32 revisionHeight) view returns(bytes32)
func (_Contract *ContractCallerSession) GetConsensusStateHash(revisionHeight uint32) ([32]byte, error) {
	return _Contract.Contract.GetConsensusStateHash(&_Contract.CallOpts, revisionHeight)
}

// Misbehaviour is a free data retrieval call binding the contract method 0xddba6537.
//
// Solidity: function misbehaviour(bytes ) pure returns()
func (_Contract *ContractCaller) Misbehaviour(opts *bind.CallOpts, arg0 []byte) error {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "misbehaviour", arg0)

	if err != nil {
		return err
	}

	return err

}

// Misbehaviour is a free data retrieval call binding the contract method 0xddba6537.
//
// Solidity: function misbehaviour(bytes ) pure returns()
func (_Contract *ContractSession) Misbehaviour(arg0 []byte) error {
	return _Contract.Contract.Misbehaviour(&_Contract.CallOpts, arg0)
}

// Misbehaviour is a free data retrieval call binding the contract method 0xddba6537.
//
// Solidity: function misbehaviour(bytes ) pure returns()
func (_Contract *ContractCallerSession) Misbehaviour(arg0 []byte) error {
	return _Contract.Contract.Misbehaviour(&_Contract.CallOpts, arg0)
}

// UpgradeClient is a free data retrieval call binding the contract method 0x8a8e4c5d.
//
// Solidity: function upgradeClient(bytes ) pure returns()
func (_Contract *ContractCaller) UpgradeClient(opts *bind.CallOpts, arg0 []byte) error {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "upgradeClient", arg0)

	if err != nil {
		return err
	}

	return err

}

// UpgradeClient is a free data retrieval call binding the contract method 0x8a8e4c5d.
//
// Solidity: function upgradeClient(bytes ) pure returns()
func (_Contract *ContractSession) UpgradeClient(arg0 []byte) error {
	return _Contract.Contract.UpgradeClient(&_Contract.CallOpts, arg0)
}

// UpgradeClient is a free data retrieval call binding the contract method 0x8a8e4c5d.
//
// Solidity: function upgradeClient(bytes ) pure returns()
func (_Contract *ContractCallerSession) UpgradeClient(arg0 []byte) error {
	return _Contract.Contract.UpgradeClient(&_Contract.CallOpts, arg0)
}

// Membership is a paid mutator transaction binding the contract method 0x4954b4ca.
//
// Solidity: function membership((bytes,(uint32,uint32),bytes[],bytes) msgMembership) returns(uint256 timestamp)
func (_Contract *ContractTransactor) Membership(opts *bind.TransactOpts, msgMembership ILightClientMsgsMsgMembership) (*types.Transaction, error) {
	return _Contract.contract.Transact(opts, "membership", msgMembership)
}

// Membership is a paid mutator transaction binding the contract method 0x4954b4ca.
//
// Solidity: function membership((bytes,(uint32,uint32),bytes[],bytes) msgMembership) returns(uint256 timestamp)
func (_Contract *ContractSession) Membership(msgMembership ILightClientMsgsMsgMembership) (*types.Transaction, error) {
	return _Contract.Contract.Membership(&_Contract.TransactOpts, msgMembership)
}

// Membership is a paid mutator transaction binding the contract method 0x4954b4ca.
//
// Solidity: function membership((bytes,(uint32,uint32),bytes[],bytes) msgMembership) returns(uint256 timestamp)
func (_Contract *ContractTransactorSession) Membership(msgMembership ILightClientMsgsMsgMembership) (*types.Transaction, error) {
	return _Contract.Contract.Membership(&_Contract.TransactOpts, msgMembership)
}

// UpdateClient is a paid mutator transaction binding the contract method 0x0bece356.
//
// Solidity: function updateClient(bytes updateMsg) returns(uint8)
func (_Contract *ContractTransactor) UpdateClient(opts *bind.TransactOpts, updateMsg []byte) (*types.Transaction, error) {
	return _Contract.contract.Transact(opts, "updateClient", updateMsg)
}

// UpdateClient is a paid mutator transaction binding the contract method 0x0bece356.
//
// Solidity: function updateClient(bytes updateMsg) returns(uint8)
func (_Contract *ContractSession) UpdateClient(updateMsg []byte) (*types.Transaction, error) {
	return _Contract.Contract.UpdateClient(&_Contract.TransactOpts, updateMsg)
}

// UpdateClient is a paid mutator transaction binding the contract method 0x0bece356.
//
// Solidity: function updateClient(bytes updateMsg) returns(uint8)
func (_Contract *ContractTransactorSession) UpdateClient(updateMsg []byte) (*types.Transaction, error) {
	return _Contract.Contract.UpdateClient(&_Contract.TransactOpts, updateMsg)
}
