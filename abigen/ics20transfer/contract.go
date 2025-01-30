// Code generated - DO NOT EDIT.
// This file is a generated binding and any manual changes will be lost.

package ics20transfer

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

// ICS20LibDenom is an auto generated low-level Go binding around an user-defined struct.
type ICS20LibDenom struct {
	Base  string
	Trace []ICS20LibHop
}

// ICS20LibHop is an auto generated low-level Go binding around an user-defined struct.
type ICS20LibHop struct {
	PortId   string
	ClientId string
}

// IIBCAppCallbacksOnAcknowledgementPacketCallback is an auto generated low-level Go binding around an user-defined struct.
type IIBCAppCallbacksOnAcknowledgementPacketCallback struct {
	SourceClient      string
	DestinationClient string
	Sequence          uint64
	Payload           IICS26RouterMsgsPayload
	Acknowledgement   []byte
	Relayer           common.Address
}

// IIBCAppCallbacksOnRecvPacketCallback is an auto generated low-level Go binding around an user-defined struct.
type IIBCAppCallbacksOnRecvPacketCallback struct {
	SourceClient      string
	DestinationClient string
	Sequence          uint64
	Payload           IICS26RouterMsgsPayload
	Relayer           common.Address
}

// IIBCAppCallbacksOnSendPacketCallback is an auto generated low-level Go binding around an user-defined struct.
type IIBCAppCallbacksOnSendPacketCallback struct {
	SourceClient      string
	DestinationClient string
	Sequence          uint64
	Payload           IICS26RouterMsgsPayload
	Sender            common.Address
}

// IIBCAppCallbacksOnTimeoutPacketCallback is an auto generated low-level Go binding around an user-defined struct.
type IIBCAppCallbacksOnTimeoutPacketCallback struct {
	SourceClient      string
	DestinationClient string
	Sequence          uint64
	Payload           IICS26RouterMsgsPayload
	Relayer           common.Address
}

// IICS20TransferMsgsERC20Token is an auto generated low-level Go binding around an user-defined struct.
type IICS20TransferMsgsERC20Token struct {
	ContractAddress common.Address
	Amount          *big.Int
}

// IICS20TransferMsgsForwarding is an auto generated low-level Go binding around an user-defined struct.
type IICS20TransferMsgsForwarding struct {
	Hops []ICS20LibHop
}

// IICS20TransferMsgsSendTransferMsg is an auto generated low-level Go binding around an user-defined struct.
type IICS20TransferMsgsSendTransferMsg struct {
	Tokens           []IICS20TransferMsgsERC20Token
	Receiver         string
	SourceClient     string
	DestPort         string
	TimeoutTimestamp uint64
	Memo             string
	Forwarding       IICS20TransferMsgsForwarding
}

// IICS26RouterMsgsMsgSendPacket is an auto generated low-level Go binding around an user-defined struct.
type IICS26RouterMsgsMsgSendPacket struct {
	SourceClient     string
	TimeoutTimestamp uint64
	Payloads         []IICS26RouterMsgsPayload
}

// IICS26RouterMsgsPayload is an auto generated low-level Go binding around an user-defined struct.
type IICS26RouterMsgsPayload struct {
	SourcePort string
	DestPort   string
	Version    string
	Encoding   string
	Value      []byte
}

// ContractMetaData contains all meta data concerning the Contract contract.
var ContractMetaData = &bind.MetaData{
	ABI: "[{\"type\":\"constructor\",\"inputs\":[],\"stateMutability\":\"nonpayable\"},{\"type\":\"function\",\"name\":\"escrow\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"address\",\"internalType\":\"address\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"ibcERC20Contract\",\"inputs\":[{\"name\":\"denom\",\"type\":\"tuple\",\"internalType\":\"structICS20Lib.Denom\",\"components\":[{\"name\":\"base\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"trace\",\"type\":\"tuple[]\",\"internalType\":\"structICS20Lib.Hop[]\",\"components\":[{\"name\":\"portId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"clientId\",\"type\":\"string\",\"internalType\":\"string\"}]}]}],\"outputs\":[{\"name\":\"\",\"type\":\"address\",\"internalType\":\"address\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"initialize\",\"inputs\":[{\"name\":\"ics26Router\",\"type\":\"address\",\"internalType\":\"address\"}],\"outputs\":[],\"stateMutability\":\"nonpayable\"},{\"type\":\"function\",\"name\":\"multicall\",\"inputs\":[{\"name\":\"data\",\"type\":\"bytes[]\",\"internalType\":\"bytes[]\"}],\"outputs\":[{\"name\":\"results\",\"type\":\"bytes[]\",\"internalType\":\"bytes[]\"}],\"stateMutability\":\"nonpayable\"},{\"type\":\"function\",\"name\":\"newMsgSendPacketV2\",\"inputs\":[{\"name\":\"sender\",\"type\":\"address\",\"internalType\":\"address\"},{\"name\":\"msg_\",\"type\":\"tuple\",\"internalType\":\"structIICS20TransferMsgs.SendTransferMsg\",\"components\":[{\"name\":\"tokens\",\"type\":\"tuple[]\",\"internalType\":\"structIICS20TransferMsgs.ERC20Token[]\",\"components\":[{\"name\":\"contractAddress\",\"type\":\"address\",\"internalType\":\"address\"},{\"name\":\"amount\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"name\":\"receiver\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"sourceClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destPort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"timeoutTimestamp\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"memo\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"forwarding\",\"type\":\"tuple\",\"internalType\":\"structIICS20TransferMsgs.Forwarding\",\"components\":[{\"name\":\"hops\",\"type\":\"tuple[]\",\"internalType\":\"structICS20Lib.Hop[]\",\"components\":[{\"name\":\"portId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"clientId\",\"type\":\"string\",\"internalType\":\"string\"}]}]}]}],\"outputs\":[{\"name\":\"\",\"type\":\"tuple\",\"internalType\":\"structIICS26RouterMsgs.MsgSendPacket\",\"components\":[{\"name\":\"sourceClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"timeoutTimestamp\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"payloads\",\"type\":\"tuple[]\",\"internalType\":\"structIICS26RouterMsgs.Payload[]\",\"components\":[{\"name\":\"sourcePort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destPort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"version\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"encoding\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"value\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]}]}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"onAcknowledgementPacket\",\"inputs\":[{\"name\":\"msg_\",\"type\":\"tuple\",\"internalType\":\"structIIBCAppCallbacks.OnAcknowledgementPacketCallback\",\"components\":[{\"name\":\"sourceClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destinationClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"sequence\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"payload\",\"type\":\"tuple\",\"internalType\":\"structIICS26RouterMsgs.Payload\",\"components\":[{\"name\":\"sourcePort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destPort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"version\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"encoding\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"value\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]},{\"name\":\"acknowledgement\",\"type\":\"bytes\",\"internalType\":\"bytes\"},{\"name\":\"relayer\",\"type\":\"address\",\"internalType\":\"address\"}]}],\"outputs\":[],\"stateMutability\":\"nonpayable\"},{\"type\":\"function\",\"name\":\"onRecvPacket\",\"inputs\":[{\"name\":\"msg_\",\"type\":\"tuple\",\"internalType\":\"structIIBCAppCallbacks.OnRecvPacketCallback\",\"components\":[{\"name\":\"sourceClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destinationClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"sequence\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"payload\",\"type\":\"tuple\",\"internalType\":\"structIICS26RouterMsgs.Payload\",\"components\":[{\"name\":\"sourcePort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destPort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"version\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"encoding\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"value\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]},{\"name\":\"relayer\",\"type\":\"address\",\"internalType\":\"address\"}]}],\"outputs\":[{\"name\":\"\",\"type\":\"bytes\",\"internalType\":\"bytes\"}],\"stateMutability\":\"nonpayable\"},{\"type\":\"function\",\"name\":\"onSendPacket\",\"inputs\":[{\"name\":\"msg_\",\"type\":\"tuple\",\"internalType\":\"structIIBCAppCallbacks.OnSendPacketCallback\",\"components\":[{\"name\":\"sourceClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destinationClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"sequence\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"payload\",\"type\":\"tuple\",\"internalType\":\"structIICS26RouterMsgs.Payload\",\"components\":[{\"name\":\"sourcePort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destPort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"version\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"encoding\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"value\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]},{\"name\":\"sender\",\"type\":\"address\",\"internalType\":\"address\"}]}],\"outputs\":[],\"stateMutability\":\"nonpayable\"},{\"type\":\"function\",\"name\":\"onTimeoutPacket\",\"inputs\":[{\"name\":\"msg_\",\"type\":\"tuple\",\"internalType\":\"structIIBCAppCallbacks.OnTimeoutPacketCallback\",\"components\":[{\"name\":\"sourceClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destinationClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"sequence\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"payload\",\"type\":\"tuple\",\"internalType\":\"structIICS26RouterMsgs.Payload\",\"components\":[{\"name\":\"sourcePort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destPort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"version\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"encoding\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"value\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]},{\"name\":\"relayer\",\"type\":\"address\",\"internalType\":\"address\"}]}],\"outputs\":[],\"stateMutability\":\"nonpayable\"},{\"type\":\"function\",\"name\":\"sendTransfer\",\"inputs\":[{\"name\":\"msg_\",\"type\":\"tuple\",\"internalType\":\"structIICS20TransferMsgs.SendTransferMsg\",\"components\":[{\"name\":\"tokens\",\"type\":\"tuple[]\",\"internalType\":\"structIICS20TransferMsgs.ERC20Token[]\",\"components\":[{\"name\":\"contractAddress\",\"type\":\"address\",\"internalType\":\"address\"},{\"name\":\"amount\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"name\":\"receiver\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"sourceClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destPort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"timeoutTimestamp\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"memo\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"forwarding\",\"type\":\"tuple\",\"internalType\":\"structIICS20TransferMsgs.Forwarding\",\"components\":[{\"name\":\"hops\",\"type\":\"tuple[]\",\"internalType\":\"structICS20Lib.Hop[]\",\"components\":[{\"name\":\"portId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"clientId\",\"type\":\"string\",\"internalType\":\"string\"}]}]}]}],\"outputs\":[{\"name\":\"\",\"type\":\"uint32\",\"internalType\":\"uint32\"}],\"stateMutability\":\"nonpayable\"},{\"type\":\"event\",\"name\":\"Initialized\",\"inputs\":[{\"name\":\"version\",\"type\":\"uint64\",\"indexed\":false,\"internalType\":\"uint64\"}],\"anonymous\":false},{\"type\":\"error\",\"name\":\"AddressEmptyCode\",\"inputs\":[{\"name\":\"target\",\"type\":\"address\",\"internalType\":\"address\"}]},{\"type\":\"error\",\"name\":\"FailedCall\",\"inputs\":[]},{\"type\":\"error\",\"name\":\"ICS20AbiEncodingFailure\",\"inputs\":[]},{\"type\":\"error\",\"name\":\"ICS20DenomNotFound\",\"inputs\":[{\"name\":\"denom\",\"type\":\"tuple\",\"internalType\":\"structICS20Lib.Denom\",\"components\":[{\"name\":\"base\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"trace\",\"type\":\"tuple[]\",\"internalType\":\"structICS20Lib.Hop[]\",\"components\":[{\"name\":\"portId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"clientId\",\"type\":\"string\",\"internalType\":\"string\"}]}]}]},{\"type\":\"error\",\"name\":\"ICS20InvalidAddress\",\"inputs\":[{\"name\":\"addr\",\"type\":\"string\",\"internalType\":\"string\"}]},{\"type\":\"error\",\"name\":\"ICS20InvalidAmount\",\"inputs\":[{\"name\":\"amount\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"type\":\"error\",\"name\":\"ICS20Unauthorized\",\"inputs\":[{\"name\":\"caller\",\"type\":\"address\",\"internalType\":\"address\"}]},{\"type\":\"error\",\"name\":\"ICS20UnauthorizedPacketSender\",\"inputs\":[{\"name\":\"packetSender\",\"type\":\"address\",\"internalType\":\"address\"}]},{\"type\":\"error\",\"name\":\"ICS20UnexpectedERC20Balance\",\"inputs\":[{\"name\":\"expected\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"actual\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"type\":\"error\",\"name\":\"ICS20UnexpectedVersion\",\"inputs\":[{\"name\":\"expected\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"version\",\"type\":\"string\",\"internalType\":\"string\"}]},{\"type\":\"error\",\"name\":\"ICS20UnsupportedFeature\",\"inputs\":[{\"name\":\"feature\",\"type\":\"string\",\"internalType\":\"string\"}]},{\"type\":\"error\",\"name\":\"InvalidInitialization\",\"inputs\":[]},{\"type\":\"error\",\"name\":\"NotInitializing\",\"inputs\":[]},{\"type\":\"error\",\"name\":\"ReentrancyGuardReentrantCall\",\"inputs\":[]},{\"type\":\"error\",\"name\":\"SafeERC20FailedOperation\",\"inputs\":[{\"name\":\"token\",\"type\":\"address\",\"internalType\":\"address\"}]}]",
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

// Escrow is a free data retrieval call binding the contract method 0xe2fdcc17.
//
// Solidity: function escrow() view returns(address)
func (_Contract *ContractCaller) Escrow(opts *bind.CallOpts) (common.Address, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "escrow")

	if err != nil {
		return *new(common.Address), err
	}

	out0 := *abi.ConvertType(out[0], new(common.Address)).(*common.Address)

	return out0, err

}

// Escrow is a free data retrieval call binding the contract method 0xe2fdcc17.
//
// Solidity: function escrow() view returns(address)
func (_Contract *ContractSession) Escrow() (common.Address, error) {
	return _Contract.Contract.Escrow(&_Contract.CallOpts)
}

// Escrow is a free data retrieval call binding the contract method 0xe2fdcc17.
//
// Solidity: function escrow() view returns(address)
func (_Contract *ContractCallerSession) Escrow() (common.Address, error) {
	return _Contract.Contract.Escrow(&_Contract.CallOpts)
}

// IbcERC20Contract is a free data retrieval call binding the contract method 0xf74da15a.
//
// Solidity: function ibcERC20Contract((string,(string,string)[]) denom) view returns(address)
func (_Contract *ContractCaller) IbcERC20Contract(opts *bind.CallOpts, denom ICS20LibDenom) (common.Address, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "ibcERC20Contract", denom)

	if err != nil {
		return *new(common.Address), err
	}

	out0 := *abi.ConvertType(out[0], new(common.Address)).(*common.Address)

	return out0, err

}

// IbcERC20Contract is a free data retrieval call binding the contract method 0xf74da15a.
//
// Solidity: function ibcERC20Contract((string,(string,string)[]) denom) view returns(address)
func (_Contract *ContractSession) IbcERC20Contract(denom ICS20LibDenom) (common.Address, error) {
	return _Contract.Contract.IbcERC20Contract(&_Contract.CallOpts, denom)
}

// IbcERC20Contract is a free data retrieval call binding the contract method 0xf74da15a.
//
// Solidity: function ibcERC20Contract((string,(string,string)[]) denom) view returns(address)
func (_Contract *ContractCallerSession) IbcERC20Contract(denom ICS20LibDenom) (common.Address, error) {
	return _Contract.Contract.IbcERC20Contract(&_Contract.CallOpts, denom)
}

// NewMsgSendPacketV2 is a free data retrieval call binding the contract method 0x27340635.
//
// Solidity: function newMsgSendPacketV2(address sender, ((address,uint256)[],string,string,string,uint64,string,((string,string)[])) msg_) view returns((string,uint64,(string,string,string,string,bytes)[]))
func (_Contract *ContractCaller) NewMsgSendPacketV2(opts *bind.CallOpts, sender common.Address, msg_ IICS20TransferMsgsSendTransferMsg) (IICS26RouterMsgsMsgSendPacket, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "newMsgSendPacketV2", sender, msg_)

	if err != nil {
		return *new(IICS26RouterMsgsMsgSendPacket), err
	}

	out0 := *abi.ConvertType(out[0], new(IICS26RouterMsgsMsgSendPacket)).(*IICS26RouterMsgsMsgSendPacket)

	return out0, err

}

// NewMsgSendPacketV2 is a free data retrieval call binding the contract method 0x27340635.
//
// Solidity: function newMsgSendPacketV2(address sender, ((address,uint256)[],string,string,string,uint64,string,((string,string)[])) msg_) view returns((string,uint64,(string,string,string,string,bytes)[]))
func (_Contract *ContractSession) NewMsgSendPacketV2(sender common.Address, msg_ IICS20TransferMsgsSendTransferMsg) (IICS26RouterMsgsMsgSendPacket, error) {
	return _Contract.Contract.NewMsgSendPacketV2(&_Contract.CallOpts, sender, msg_)
}

// NewMsgSendPacketV2 is a free data retrieval call binding the contract method 0x27340635.
//
// Solidity: function newMsgSendPacketV2(address sender, ((address,uint256)[],string,string,string,uint64,string,((string,string)[])) msg_) view returns((string,uint64,(string,string,string,string,bytes)[]))
func (_Contract *ContractCallerSession) NewMsgSendPacketV2(sender common.Address, msg_ IICS20TransferMsgsSendTransferMsg) (IICS26RouterMsgsMsgSendPacket, error) {
	return _Contract.Contract.NewMsgSendPacketV2(&_Contract.CallOpts, sender, msg_)
}

// Initialize is a paid mutator transaction binding the contract method 0xc4d66de8.
//
// Solidity: function initialize(address ics26Router) returns()
func (_Contract *ContractTransactor) Initialize(opts *bind.TransactOpts, ics26Router common.Address) (*types.Transaction, error) {
	return _Contract.contract.Transact(opts, "initialize", ics26Router)
}

// Initialize is a paid mutator transaction binding the contract method 0xc4d66de8.
//
// Solidity: function initialize(address ics26Router) returns()
func (_Contract *ContractSession) Initialize(ics26Router common.Address) (*types.Transaction, error) {
	return _Contract.Contract.Initialize(&_Contract.TransactOpts, ics26Router)
}

// Initialize is a paid mutator transaction binding the contract method 0xc4d66de8.
//
// Solidity: function initialize(address ics26Router) returns()
func (_Contract *ContractTransactorSession) Initialize(ics26Router common.Address) (*types.Transaction, error) {
	return _Contract.Contract.Initialize(&_Contract.TransactOpts, ics26Router)
}

// Multicall is a paid mutator transaction binding the contract method 0xac9650d8.
//
// Solidity: function multicall(bytes[] data) returns(bytes[] results)
func (_Contract *ContractTransactor) Multicall(opts *bind.TransactOpts, data [][]byte) (*types.Transaction, error) {
	return _Contract.contract.Transact(opts, "multicall", data)
}

// Multicall is a paid mutator transaction binding the contract method 0xac9650d8.
//
// Solidity: function multicall(bytes[] data) returns(bytes[] results)
func (_Contract *ContractSession) Multicall(data [][]byte) (*types.Transaction, error) {
	return _Contract.Contract.Multicall(&_Contract.TransactOpts, data)
}

// Multicall is a paid mutator transaction binding the contract method 0xac9650d8.
//
// Solidity: function multicall(bytes[] data) returns(bytes[] results)
func (_Contract *ContractTransactorSession) Multicall(data [][]byte) (*types.Transaction, error) {
	return _Contract.Contract.Multicall(&_Contract.TransactOpts, data)
}

// OnAcknowledgementPacket is a paid mutator transaction binding the contract method 0x428e4e17.
//
// Solidity: function onAcknowledgementPacket((string,string,uint64,(string,string,string,string,bytes),bytes,address) msg_) returns()
func (_Contract *ContractTransactor) OnAcknowledgementPacket(opts *bind.TransactOpts, msg_ IIBCAppCallbacksOnAcknowledgementPacketCallback) (*types.Transaction, error) {
	return _Contract.contract.Transact(opts, "onAcknowledgementPacket", msg_)
}

// OnAcknowledgementPacket is a paid mutator transaction binding the contract method 0x428e4e17.
//
// Solidity: function onAcknowledgementPacket((string,string,uint64,(string,string,string,string,bytes),bytes,address) msg_) returns()
func (_Contract *ContractSession) OnAcknowledgementPacket(msg_ IIBCAppCallbacksOnAcknowledgementPacketCallback) (*types.Transaction, error) {
	return _Contract.Contract.OnAcknowledgementPacket(&_Contract.TransactOpts, msg_)
}

// OnAcknowledgementPacket is a paid mutator transaction binding the contract method 0x428e4e17.
//
// Solidity: function onAcknowledgementPacket((string,string,uint64,(string,string,string,string,bytes),bytes,address) msg_) returns()
func (_Contract *ContractTransactorSession) OnAcknowledgementPacket(msg_ IIBCAppCallbacksOnAcknowledgementPacketCallback) (*types.Transaction, error) {
	return _Contract.Contract.OnAcknowledgementPacket(&_Contract.TransactOpts, msg_)
}

// OnRecvPacket is a paid mutator transaction binding the contract method 0x078c4a79.
//
// Solidity: function onRecvPacket((string,string,uint64,(string,string,string,string,bytes),address) msg_) returns(bytes)
func (_Contract *ContractTransactor) OnRecvPacket(opts *bind.TransactOpts, msg_ IIBCAppCallbacksOnRecvPacketCallback) (*types.Transaction, error) {
	return _Contract.contract.Transact(opts, "onRecvPacket", msg_)
}

// OnRecvPacket is a paid mutator transaction binding the contract method 0x078c4a79.
//
// Solidity: function onRecvPacket((string,string,uint64,(string,string,string,string,bytes),address) msg_) returns(bytes)
func (_Contract *ContractSession) OnRecvPacket(msg_ IIBCAppCallbacksOnRecvPacketCallback) (*types.Transaction, error) {
	return _Contract.Contract.OnRecvPacket(&_Contract.TransactOpts, msg_)
}

// OnRecvPacket is a paid mutator transaction binding the contract method 0x078c4a79.
//
// Solidity: function onRecvPacket((string,string,uint64,(string,string,string,string,bytes),address) msg_) returns(bytes)
func (_Contract *ContractTransactorSession) OnRecvPacket(msg_ IIBCAppCallbacksOnRecvPacketCallback) (*types.Transaction, error) {
	return _Contract.Contract.OnRecvPacket(&_Contract.TransactOpts, msg_)
}

// OnSendPacket is a paid mutator transaction binding the contract method 0x8356f124.
//
// Solidity: function onSendPacket((string,string,uint64,(string,string,string,string,bytes),address) msg_) returns()
func (_Contract *ContractTransactor) OnSendPacket(opts *bind.TransactOpts, msg_ IIBCAppCallbacksOnSendPacketCallback) (*types.Transaction, error) {
	return _Contract.contract.Transact(opts, "onSendPacket", msg_)
}

// OnSendPacket is a paid mutator transaction binding the contract method 0x8356f124.
//
// Solidity: function onSendPacket((string,string,uint64,(string,string,string,string,bytes),address) msg_) returns()
func (_Contract *ContractSession) OnSendPacket(msg_ IIBCAppCallbacksOnSendPacketCallback) (*types.Transaction, error) {
	return _Contract.Contract.OnSendPacket(&_Contract.TransactOpts, msg_)
}

// OnSendPacket is a paid mutator transaction binding the contract method 0x8356f124.
//
// Solidity: function onSendPacket((string,string,uint64,(string,string,string,string,bytes),address) msg_) returns()
func (_Contract *ContractTransactorSession) OnSendPacket(msg_ IIBCAppCallbacksOnSendPacketCallback) (*types.Transaction, error) {
	return _Contract.Contract.OnSendPacket(&_Contract.TransactOpts, msg_)
}

// OnTimeoutPacket is a paid mutator transaction binding the contract method 0x5e32b6b6.
//
// Solidity: function onTimeoutPacket((string,string,uint64,(string,string,string,string,bytes),address) msg_) returns()
func (_Contract *ContractTransactor) OnTimeoutPacket(opts *bind.TransactOpts, msg_ IIBCAppCallbacksOnTimeoutPacketCallback) (*types.Transaction, error) {
	return _Contract.contract.Transact(opts, "onTimeoutPacket", msg_)
}

// OnTimeoutPacket is a paid mutator transaction binding the contract method 0x5e32b6b6.
//
// Solidity: function onTimeoutPacket((string,string,uint64,(string,string,string,string,bytes),address) msg_) returns()
func (_Contract *ContractSession) OnTimeoutPacket(msg_ IIBCAppCallbacksOnTimeoutPacketCallback) (*types.Transaction, error) {
	return _Contract.Contract.OnTimeoutPacket(&_Contract.TransactOpts, msg_)
}

// OnTimeoutPacket is a paid mutator transaction binding the contract method 0x5e32b6b6.
//
// Solidity: function onTimeoutPacket((string,string,uint64,(string,string,string,string,bytes),address) msg_) returns()
func (_Contract *ContractTransactorSession) OnTimeoutPacket(msg_ IIBCAppCallbacksOnTimeoutPacketCallback) (*types.Transaction, error) {
	return _Contract.Contract.OnTimeoutPacket(&_Contract.TransactOpts, msg_)
}

// SendTransfer is a paid mutator transaction binding the contract method 0x7fff246a.
//
// Solidity: function sendTransfer(((address,uint256)[],string,string,string,uint64,string,((string,string)[])) msg_) returns(uint32)
func (_Contract *ContractTransactor) SendTransfer(opts *bind.TransactOpts, msg_ IICS20TransferMsgsSendTransferMsg) (*types.Transaction, error) {
	return _Contract.contract.Transact(opts, "sendTransfer", msg_)
}

// SendTransfer is a paid mutator transaction binding the contract method 0x7fff246a.
//
// Solidity: function sendTransfer(((address,uint256)[],string,string,string,uint64,string,((string,string)[])) msg_) returns(uint32)
func (_Contract *ContractSession) SendTransfer(msg_ IICS20TransferMsgsSendTransferMsg) (*types.Transaction, error) {
	return _Contract.Contract.SendTransfer(&_Contract.TransactOpts, msg_)
}

// SendTransfer is a paid mutator transaction binding the contract method 0x7fff246a.
//
// Solidity: function sendTransfer(((address,uint256)[],string,string,string,uint64,string,((string,string)[])) msg_) returns(uint32)
func (_Contract *ContractTransactorSession) SendTransfer(msg_ IICS20TransferMsgsSendTransferMsg) (*types.Transaction, error) {
	return _Contract.Contract.SendTransfer(&_Contract.TransactOpts, msg_)
}

// ContractInitializedIterator is returned from FilterInitialized and is used to iterate over the raw logs and unpacked data for Initialized events raised by the Contract contract.
type ContractInitializedIterator struct {
	Event *ContractInitialized // Event containing the contract specifics and raw log

	contract *bind.BoundContract // Generic contract to use for unpacking event data
	event    string              // Event name to use for unpacking event data

	logs chan types.Log        // Log channel receiving the found contract events
	sub  ethereum.Subscription // Subscription for errors, completion and termination
	done bool                  // Whether the subscription completed delivering logs
	fail error                 // Occurred error to stop iteration
}

// Next advances the iterator to the subsequent event, returning whether there
// are any more events found. In case of a retrieval or parsing error, false is
// returned and Error() can be queried for the exact failure.
func (it *ContractInitializedIterator) Next() bool {
	// If the iterator failed, stop iterating
	if it.fail != nil {
		return false
	}
	// If the iterator completed, deliver directly whatever's available
	if it.done {
		select {
		case log := <-it.logs:
			it.Event = new(ContractInitialized)
			if err := it.contract.UnpackLog(it.Event, it.event, log); err != nil {
				it.fail = err
				return false
			}
			it.Event.Raw = log
			return true

		default:
			return false
		}
	}
	// Iterator still in progress, wait for either a data or an error event
	select {
	case log := <-it.logs:
		it.Event = new(ContractInitialized)
		if err := it.contract.UnpackLog(it.Event, it.event, log); err != nil {
			it.fail = err
			return false
		}
		it.Event.Raw = log
		return true

	case err := <-it.sub.Err():
		it.done = true
		it.fail = err
		return it.Next()
	}
}

// Error returns any retrieval or parsing error occurred during filtering.
func (it *ContractInitializedIterator) Error() error {
	return it.fail
}

// Close terminates the iteration process, releasing any pending underlying
// resources.
func (it *ContractInitializedIterator) Close() error {
	it.sub.Unsubscribe()
	return nil
}

// ContractInitialized represents a Initialized event raised by the Contract contract.
type ContractInitialized struct {
	Version uint64
	Raw     types.Log // Blockchain specific contextual infos
}

// FilterInitialized is a free log retrieval operation binding the contract event 0xc7f505b2f371ae2175ee4913f4499e1f2633a7b5936321eed1cdaeb6115181d2.
//
// Solidity: event Initialized(uint64 version)
func (_Contract *ContractFilterer) FilterInitialized(opts *bind.FilterOpts) (*ContractInitializedIterator, error) {

	logs, sub, err := _Contract.contract.FilterLogs(opts, "Initialized")
	if err != nil {
		return nil, err
	}
	return &ContractInitializedIterator{contract: _Contract.contract, event: "Initialized", logs: logs, sub: sub}, nil
}

// WatchInitialized is a free log subscription operation binding the contract event 0xc7f505b2f371ae2175ee4913f4499e1f2633a7b5936321eed1cdaeb6115181d2.
//
// Solidity: event Initialized(uint64 version)
func (_Contract *ContractFilterer) WatchInitialized(opts *bind.WatchOpts, sink chan<- *ContractInitialized) (event.Subscription, error) {

	logs, sub, err := _Contract.contract.WatchLogs(opts, "Initialized")
	if err != nil {
		return nil, err
	}
	return event.NewSubscription(func(quit <-chan struct{}) error {
		defer sub.Unsubscribe()
		for {
			select {
			case log := <-logs:
				// New log arrived, parse the event and forward to the user
				event := new(ContractInitialized)
				if err := _Contract.contract.UnpackLog(event, "Initialized", log); err != nil {
					return err
				}
				event.Raw = log

				select {
				case sink <- event:
				case err := <-sub.Err():
					return err
				case <-quit:
					return nil
				}
			case err := <-sub.Err():
				return err
			case <-quit:
				return nil
			}
		}
	}), nil
}

// ParseInitialized is a log parse operation binding the contract event 0xc7f505b2f371ae2175ee4913f4499e1f2633a7b5936321eed1cdaeb6115181d2.
//
// Solidity: event Initialized(uint64 version)
func (_Contract *ContractFilterer) ParseInitialized(log types.Log) (*ContractInitialized, error) {
	event := new(ContractInitialized)
	if err := _Contract.contract.UnpackLog(event, "Initialized", log); err != nil {
		return nil, err
	}
	event.Raw = log
	return event, nil
}
