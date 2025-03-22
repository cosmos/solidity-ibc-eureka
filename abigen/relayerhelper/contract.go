// Code generated - DO NOT EDIT.
// This file is a generated binding and any manual changes will be lost.

package relayerhelper

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

// IICS26RouterMsgsPacket is an auto generated low-level Go binding around an user-defined struct.
type IICS26RouterMsgsPacket struct {
	Sequence         uint64
	SourceClient     string
	DestClient       string
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
	ABI: "[{\"type\":\"constructor\",\"inputs\":[{\"name\":\"_ics26Router\",\"type\":\"address\",\"internalType\":\"address\"}],\"stateMutability\":\"nonpayable\"},{\"type\":\"function\",\"name\":\"ICS26_ROUTER\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"address\",\"internalType\":\"address\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"isPacketReceiveSuccessful\",\"inputs\":[{\"name\":\"packet\",\"type\":\"tuple\",\"internalType\":\"structIICS26RouterMsgs.Packet\",\"components\":[{\"name\":\"sequence\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"sourceClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"timeoutTimestamp\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"payloads\",\"type\":\"tuple[]\",\"internalType\":\"structIICS26RouterMsgs.Payload[]\",\"components\":[{\"name\":\"sourcePort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destPort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"version\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"encoding\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"value\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]}]}],\"outputs\":[{\"name\":\"\",\"type\":\"bool\",\"internalType\":\"bool\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"isPacketReceived\",\"inputs\":[{\"name\":\"packet\",\"type\":\"tuple\",\"internalType\":\"structIICS26RouterMsgs.Packet\",\"components\":[{\"name\":\"sequence\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"sourceClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"timeoutTimestamp\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"payloads\",\"type\":\"tuple[]\",\"internalType\":\"structIICS26RouterMsgs.Payload[]\",\"components\":[{\"name\":\"sourcePort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destPort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"version\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"encoding\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"value\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]}]}],\"outputs\":[{\"name\":\"\",\"type\":\"bool\",\"internalType\":\"bool\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"queryAckCommitment\",\"inputs\":[{\"name\":\"clientId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"sequence\",\"type\":\"uint64\",\"internalType\":\"uint64\"}],\"outputs\":[{\"name\":\"\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"queryPacketCommitment\",\"inputs\":[{\"name\":\"clientId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"sequence\",\"type\":\"uint64\",\"internalType\":\"uint64\"}],\"outputs\":[{\"name\":\"\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"queryPacketReceipt\",\"inputs\":[{\"name\":\"clientId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"sequence\",\"type\":\"uint64\",\"internalType\":\"uint64\"}],\"outputs\":[{\"name\":\"\",\"type\":\"bytes32\",\"internalType\":\"bytes32\"}],\"stateMutability\":\"view\"},{\"type\":\"error\",\"name\":\"NoAcknowledgements\",\"inputs\":[]}]",
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

// ICS26ROUTER is a free data retrieval call binding the contract method 0xc20f00e1.
//
// Solidity: function ICS26_ROUTER() view returns(address)
func (_Contract *ContractCaller) ICS26ROUTER(opts *bind.CallOpts) (common.Address, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "ICS26_ROUTER")

	if err != nil {
		return *new(common.Address), err
	}

	out0 := *abi.ConvertType(out[0], new(common.Address)).(*common.Address)

	return out0, err

}

// ICS26ROUTER is a free data retrieval call binding the contract method 0xc20f00e1.
//
// Solidity: function ICS26_ROUTER() view returns(address)
func (_Contract *ContractSession) ICS26ROUTER() (common.Address, error) {
	return _Contract.Contract.ICS26ROUTER(&_Contract.CallOpts)
}

// ICS26ROUTER is a free data retrieval call binding the contract method 0xc20f00e1.
//
// Solidity: function ICS26_ROUTER() view returns(address)
func (_Contract *ContractCallerSession) ICS26ROUTER() (common.Address, error) {
	return _Contract.Contract.ICS26ROUTER(&_Contract.CallOpts)
}

// IsPacketReceiveSuccessful is a free data retrieval call binding the contract method 0xede05f16.
//
// Solidity: function isPacketReceiveSuccessful((uint64,string,string,uint64,(string,string,string,string,bytes)[]) packet) view returns(bool)
func (_Contract *ContractCaller) IsPacketReceiveSuccessful(opts *bind.CallOpts, packet IICS26RouterMsgsPacket) (bool, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "isPacketReceiveSuccessful", packet)

	if err != nil {
		return *new(bool), err
	}

	out0 := *abi.ConvertType(out[0], new(bool)).(*bool)

	return out0, err

}

// IsPacketReceiveSuccessful is a free data retrieval call binding the contract method 0xede05f16.
//
// Solidity: function isPacketReceiveSuccessful((uint64,string,string,uint64,(string,string,string,string,bytes)[]) packet) view returns(bool)
func (_Contract *ContractSession) IsPacketReceiveSuccessful(packet IICS26RouterMsgsPacket) (bool, error) {
	return _Contract.Contract.IsPacketReceiveSuccessful(&_Contract.CallOpts, packet)
}

// IsPacketReceiveSuccessful is a free data retrieval call binding the contract method 0xede05f16.
//
// Solidity: function isPacketReceiveSuccessful((uint64,string,string,uint64,(string,string,string,string,bytes)[]) packet) view returns(bool)
func (_Contract *ContractCallerSession) IsPacketReceiveSuccessful(packet IICS26RouterMsgsPacket) (bool, error) {
	return _Contract.Contract.IsPacketReceiveSuccessful(&_Contract.CallOpts, packet)
}

// IsPacketReceived is a free data retrieval call binding the contract method 0x7ecfb0c2.
//
// Solidity: function isPacketReceived((uint64,string,string,uint64,(string,string,string,string,bytes)[]) packet) view returns(bool)
func (_Contract *ContractCaller) IsPacketReceived(opts *bind.CallOpts, packet IICS26RouterMsgsPacket) (bool, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "isPacketReceived", packet)

	if err != nil {
		return *new(bool), err
	}

	out0 := *abi.ConvertType(out[0], new(bool)).(*bool)

	return out0, err

}

// IsPacketReceived is a free data retrieval call binding the contract method 0x7ecfb0c2.
//
// Solidity: function isPacketReceived((uint64,string,string,uint64,(string,string,string,string,bytes)[]) packet) view returns(bool)
func (_Contract *ContractSession) IsPacketReceived(packet IICS26RouterMsgsPacket) (bool, error) {
	return _Contract.Contract.IsPacketReceived(&_Contract.CallOpts, packet)
}

// IsPacketReceived is a free data retrieval call binding the contract method 0x7ecfb0c2.
//
// Solidity: function isPacketReceived((uint64,string,string,uint64,(string,string,string,string,bytes)[]) packet) view returns(bool)
func (_Contract *ContractCallerSession) IsPacketReceived(packet IICS26RouterMsgsPacket) (bool, error) {
	return _Contract.Contract.IsPacketReceived(&_Contract.CallOpts, packet)
}

// QueryAckCommitment is a free data retrieval call binding the contract method 0xdb8b4b63.
//
// Solidity: function queryAckCommitment(string clientId, uint64 sequence) view returns(bytes32)
func (_Contract *ContractCaller) QueryAckCommitment(opts *bind.CallOpts, clientId string, sequence uint64) ([32]byte, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "queryAckCommitment", clientId, sequence)

	if err != nil {
		return *new([32]byte), err
	}

	out0 := *abi.ConvertType(out[0], new([32]byte)).(*[32]byte)

	return out0, err

}

// QueryAckCommitment is a free data retrieval call binding the contract method 0xdb8b4b63.
//
// Solidity: function queryAckCommitment(string clientId, uint64 sequence) view returns(bytes32)
func (_Contract *ContractSession) QueryAckCommitment(clientId string, sequence uint64) ([32]byte, error) {
	return _Contract.Contract.QueryAckCommitment(&_Contract.CallOpts, clientId, sequence)
}

// QueryAckCommitment is a free data retrieval call binding the contract method 0xdb8b4b63.
//
// Solidity: function queryAckCommitment(string clientId, uint64 sequence) view returns(bytes32)
func (_Contract *ContractCallerSession) QueryAckCommitment(clientId string, sequence uint64) ([32]byte, error) {
	return _Contract.Contract.QueryAckCommitment(&_Contract.CallOpts, clientId, sequence)
}

// QueryPacketCommitment is a free data retrieval call binding the contract method 0xdfdcf16d.
//
// Solidity: function queryPacketCommitment(string clientId, uint64 sequence) view returns(bytes32)
func (_Contract *ContractCaller) QueryPacketCommitment(opts *bind.CallOpts, clientId string, sequence uint64) ([32]byte, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "queryPacketCommitment", clientId, sequence)

	if err != nil {
		return *new([32]byte), err
	}

	out0 := *abi.ConvertType(out[0], new([32]byte)).(*[32]byte)

	return out0, err

}

// QueryPacketCommitment is a free data retrieval call binding the contract method 0xdfdcf16d.
//
// Solidity: function queryPacketCommitment(string clientId, uint64 sequence) view returns(bytes32)
func (_Contract *ContractSession) QueryPacketCommitment(clientId string, sequence uint64) ([32]byte, error) {
	return _Contract.Contract.QueryPacketCommitment(&_Contract.CallOpts, clientId, sequence)
}

// QueryPacketCommitment is a free data retrieval call binding the contract method 0xdfdcf16d.
//
// Solidity: function queryPacketCommitment(string clientId, uint64 sequence) view returns(bytes32)
func (_Contract *ContractCallerSession) QueryPacketCommitment(clientId string, sequence uint64) ([32]byte, error) {
	return _Contract.Contract.QueryPacketCommitment(&_Contract.CallOpts, clientId, sequence)
}

// QueryPacketReceipt is a free data retrieval call binding the contract method 0x0bb1774c.
//
// Solidity: function queryPacketReceipt(string clientId, uint64 sequence) view returns(bytes32)
func (_Contract *ContractCaller) QueryPacketReceipt(opts *bind.CallOpts, clientId string, sequence uint64) ([32]byte, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "queryPacketReceipt", clientId, sequence)

	if err != nil {
		return *new([32]byte), err
	}

	out0 := *abi.ConvertType(out[0], new([32]byte)).(*[32]byte)

	return out0, err

}

// QueryPacketReceipt is a free data retrieval call binding the contract method 0x0bb1774c.
//
// Solidity: function queryPacketReceipt(string clientId, uint64 sequence) view returns(bytes32)
func (_Contract *ContractSession) QueryPacketReceipt(clientId string, sequence uint64) ([32]byte, error) {
	return _Contract.Contract.QueryPacketReceipt(&_Contract.CallOpts, clientId, sequence)
}

// QueryPacketReceipt is a free data retrieval call binding the contract method 0x0bb1774c.
//
// Solidity: function queryPacketReceipt(string clientId, uint64 sequence) view returns(bytes32)
func (_Contract *ContractCallerSession) QueryPacketReceipt(clientId string, sequence uint64) ([32]byte, error) {
	return _Contract.Contract.QueryPacketReceipt(&_Contract.CallOpts, clientId, sequence)
}
