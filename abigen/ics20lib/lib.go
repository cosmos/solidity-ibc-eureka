// Code generated - DO NOT EDIT.
// This file is a generated binding and any manual changes will be lost.

package ics20lib

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

// ICS20LibForwardingPacketData is an auto generated low-level Go binding around an user-defined struct.
type ICS20LibForwardingPacketData struct {
	DestinationMemo string
	Hops            []ICS20LibHop
}

// ICS20LibFungibleTokenPacketDataV2 is an auto generated low-level Go binding around an user-defined struct.
type ICS20LibFungibleTokenPacketDataV2 struct {
	Tokens     []ICS20LibToken
	Sender     string
	Receiver   string
	Memo       string
	Forwarding ICS20LibForwardingPacketData
}

// ICS20LibHop is an auto generated low-level Go binding around an user-defined struct.
type ICS20LibHop struct {
	PortId   string
	ClientId string
}

// ICS20LibToken is an auto generated low-level Go binding around an user-defined struct.
type ICS20LibToken struct {
	Denom  ICS20LibDenom
	Amount *big.Int
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

// LibMetaData contains all meta data concerning the Lib contract.
var LibMetaData = &bind.MetaData{
	ABI: "[{\"type\":\"function\",\"name\":\"DEFAULT_PORT_ID\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"string\",\"internalType\":\"string\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"FAILED_ACKNOWLEDGEMENT_JSON\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"bytes\",\"internalType\":\"bytes\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"ICS20_ENCODING\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"string\",\"internalType\":\"string\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"ICS20_VERSION\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"string\",\"internalType\":\"string\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"SUCCESSFUL_ACKNOWLEDGEMENT_JSON\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"bytes\",\"internalType\":\"bytes\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"abiPublicTypes\",\"inputs\":[{\"name\":\"o1\",\"type\":\"tuple\",\"internalType\":\"structICS20Lib.FungibleTokenPacketDataV2\",\"components\":[{\"name\":\"tokens\",\"type\":\"tuple[]\",\"internalType\":\"structICS20Lib.Token[]\",\"components\":[{\"name\":\"denom\",\"type\":\"tuple\",\"internalType\":\"structICS20Lib.Denom\",\"components\":[{\"name\":\"base\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"trace\",\"type\":\"tuple[]\",\"internalType\":\"structICS20Lib.Hop[]\",\"components\":[{\"name\":\"portId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"clientId\",\"type\":\"string\",\"internalType\":\"string\"}]}]},{\"name\":\"amount\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"name\":\"sender\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"receiver\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"memo\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"forwarding\",\"type\":\"tuple\",\"internalType\":\"structICS20Lib.ForwardingPacketData\",\"components\":[{\"name\":\"destinationMemo\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"hops\",\"type\":\"tuple[]\",\"internalType\":\"structICS20Lib.Hop[]\",\"components\":[{\"name\":\"portId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"clientId\",\"type\":\"string\",\"internalType\":\"string\"}]}]}]}],\"outputs\":[],\"stateMutability\":\"pure\"},{\"type\":\"function\",\"name\":\"getPath\",\"inputs\":[{\"name\":\"denom\",\"type\":\"tuple\",\"internalType\":\"structICS20Lib.Denom\",\"components\":[{\"name\":\"base\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"trace\",\"type\":\"tuple[]\",\"internalType\":\"structICS20Lib.Hop[]\",\"components\":[{\"name\":\"portId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"clientId\",\"type\":\"string\",\"internalType\":\"string\"}]}]}],\"outputs\":[{\"name\":\"\",\"type\":\"string\",\"internalType\":\"string\"}],\"stateMutability\":\"pure\"},{\"type\":\"function\",\"name\":\"newMsgSendPacketV2\",\"inputs\":[{\"name\":\"sender\",\"type\":\"address\",\"internalType\":\"address\"},{\"name\":\"msg_\",\"type\":\"tuple\",\"internalType\":\"structIICS20TransferMsgs.SendTransferMsg\",\"components\":[{\"name\":\"tokens\",\"type\":\"tuple[]\",\"internalType\":\"structIICS20TransferMsgs.ERC20Token[]\",\"components\":[{\"name\":\"contractAddress\",\"type\":\"address\",\"internalType\":\"address\"},{\"name\":\"amount\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"name\":\"receiver\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"sourceClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destPort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"timeoutTimestamp\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"memo\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"forwarding\",\"type\":\"tuple\",\"internalType\":\"structIICS20TransferMsgs.Forwarding\",\"components\":[{\"name\":\"hops\",\"type\":\"tuple[]\",\"internalType\":\"structICS20Lib.Hop[]\",\"components\":[{\"name\":\"portId\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"clientId\",\"type\":\"string\",\"internalType\":\"string\"}]}]}]}],\"outputs\":[{\"name\":\"\",\"type\":\"tuple\",\"internalType\":\"structIICS26RouterMsgs.MsgSendPacket\",\"components\":[{\"name\":\"sourceClient\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"timeoutTimestamp\",\"type\":\"uint64\",\"internalType\":\"uint64\"},{\"name\":\"payloads\",\"type\":\"tuple[]\",\"internalType\":\"structIICS26RouterMsgs.Payload[]\",\"components\":[{\"name\":\"sourcePort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"destPort\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"version\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"encoding\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"value\",\"type\":\"bytes\",\"internalType\":\"bytes\"}]}]}],\"stateMutability\":\"view\"},{\"type\":\"error\",\"name\":\"ICS20InvalidAmount\",\"inputs\":[{\"name\":\"amount\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]},{\"type\":\"error\",\"name\":\"StringsInsufficientHexLength\",\"inputs\":[{\"name\":\"value\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"length\",\"type\":\"uint256\",\"internalType\":\"uint256\"}]}]",
}

// LibABI is the input ABI used to generate the binding from.
// Deprecated: Use LibMetaData.ABI instead.
var LibABI = LibMetaData.ABI

// Lib is an auto generated Go binding around an Ethereum contract.
type Lib struct {
	LibCaller     // Read-only binding to the contract
	LibTransactor // Write-only binding to the contract
	LibFilterer   // Log filterer for contract events
}

// LibCaller is an auto generated read-only Go binding around an Ethereum contract.
type LibCaller struct {
	contract *bind.BoundContract // Generic contract wrapper for the low level calls
}

// LibTransactor is an auto generated write-only Go binding around an Ethereum contract.
type LibTransactor struct {
	contract *bind.BoundContract // Generic contract wrapper for the low level calls
}

// LibFilterer is an auto generated log filtering Go binding around an Ethereum contract events.
type LibFilterer struct {
	contract *bind.BoundContract // Generic contract wrapper for the low level calls
}

// LibSession is an auto generated Go binding around an Ethereum contract,
// with pre-set call and transact options.
type LibSession struct {
	Contract     *Lib              // Generic contract binding to set the session for
	CallOpts     bind.CallOpts     // Call options to use throughout this session
	TransactOpts bind.TransactOpts // Transaction auth options to use throughout this session
}

// LibCallerSession is an auto generated read-only Go binding around an Ethereum contract,
// with pre-set call options.
type LibCallerSession struct {
	Contract *LibCaller    // Generic contract caller binding to set the session for
	CallOpts bind.CallOpts // Call options to use throughout this session
}

// LibTransactorSession is an auto generated write-only Go binding around an Ethereum contract,
// with pre-set transact options.
type LibTransactorSession struct {
	Contract     *LibTransactor    // Generic contract transactor binding to set the session for
	TransactOpts bind.TransactOpts // Transaction auth options to use throughout this session
}

// LibRaw is an auto generated low-level Go binding around an Ethereum contract.
type LibRaw struct {
	Contract *Lib // Generic contract binding to access the raw methods on
}

// LibCallerRaw is an auto generated low-level read-only Go binding around an Ethereum contract.
type LibCallerRaw struct {
	Contract *LibCaller // Generic read-only contract binding to access the raw methods on
}

// LibTransactorRaw is an auto generated low-level write-only Go binding around an Ethereum contract.
type LibTransactorRaw struct {
	Contract *LibTransactor // Generic write-only contract binding to access the raw methods on
}

// NewLib creates a new instance of Lib, bound to a specific deployed contract.
func NewLib(address common.Address, backend bind.ContractBackend) (*Lib, error) {
	contract, err := bindLib(address, backend, backend, backend)
	if err != nil {
		return nil, err
	}
	return &Lib{LibCaller: LibCaller{contract: contract}, LibTransactor: LibTransactor{contract: contract}, LibFilterer: LibFilterer{contract: contract}}, nil
}

// NewLibCaller creates a new read-only instance of Lib, bound to a specific deployed contract.
func NewLibCaller(address common.Address, caller bind.ContractCaller) (*LibCaller, error) {
	contract, err := bindLib(address, caller, nil, nil)
	if err != nil {
		return nil, err
	}
	return &LibCaller{contract: contract}, nil
}

// NewLibTransactor creates a new write-only instance of Lib, bound to a specific deployed contract.
func NewLibTransactor(address common.Address, transactor bind.ContractTransactor) (*LibTransactor, error) {
	contract, err := bindLib(address, nil, transactor, nil)
	if err != nil {
		return nil, err
	}
	return &LibTransactor{contract: contract}, nil
}

// NewLibFilterer creates a new log filterer instance of Lib, bound to a specific deployed contract.
func NewLibFilterer(address common.Address, filterer bind.ContractFilterer) (*LibFilterer, error) {
	contract, err := bindLib(address, nil, nil, filterer)
	if err != nil {
		return nil, err
	}
	return &LibFilterer{contract: contract}, nil
}

// bindLib binds a generic wrapper to an already deployed contract.
func bindLib(address common.Address, caller bind.ContractCaller, transactor bind.ContractTransactor, filterer bind.ContractFilterer) (*bind.BoundContract, error) {
	parsed, err := LibMetaData.GetAbi()
	if err != nil {
		return nil, err
	}
	return bind.NewBoundContract(address, *parsed, caller, transactor, filterer), nil
}

// Call invokes the (constant) contract method with params as input values and
// sets the output to result. The result type might be a single field for simple
// returns, a slice of interfaces for anonymous returns and a struct for named
// returns.
func (_Lib *LibRaw) Call(opts *bind.CallOpts, result *[]interface{}, method string, params ...interface{}) error {
	return _Lib.Contract.LibCaller.contract.Call(opts, result, method, params...)
}

// Transfer initiates a plain transaction to move funds to the contract, calling
// its default method if one is available.
func (_Lib *LibRaw) Transfer(opts *bind.TransactOpts) (*types.Transaction, error) {
	return _Lib.Contract.LibTransactor.contract.Transfer(opts)
}

// Transact invokes the (paid) contract method with params as input values.
func (_Lib *LibRaw) Transact(opts *bind.TransactOpts, method string, params ...interface{}) (*types.Transaction, error) {
	return _Lib.Contract.LibTransactor.contract.Transact(opts, method, params...)
}

// Call invokes the (constant) contract method with params as input values and
// sets the output to result. The result type might be a single field for simple
// returns, a slice of interfaces for anonymous returns and a struct for named
// returns.
func (_Lib *LibCallerRaw) Call(opts *bind.CallOpts, result *[]interface{}, method string, params ...interface{}) error {
	return _Lib.Contract.contract.Call(opts, result, method, params...)
}

// Transfer initiates a plain transaction to move funds to the contract, calling
// its default method if one is available.
func (_Lib *LibTransactorRaw) Transfer(opts *bind.TransactOpts) (*types.Transaction, error) {
	return _Lib.Contract.contract.Transfer(opts)
}

// Transact invokes the (paid) contract method with params as input values.
func (_Lib *LibTransactorRaw) Transact(opts *bind.TransactOpts, method string, params ...interface{}) (*types.Transaction, error) {
	return _Lib.Contract.contract.Transact(opts, method, params...)
}

// DEFAULTPORTID is a free data retrieval call binding the contract method 0x78e8e9fb.
//
// Solidity: function DEFAULT_PORT_ID() view returns(string)
func (_Lib *LibCaller) DEFAULTPORTID(opts *bind.CallOpts) (string, error) {
	var out []interface{}
	err := _Lib.contract.Call(opts, &out, "DEFAULT_PORT_ID")

	if err != nil {
		return *new(string), err
	}

	out0 := *abi.ConvertType(out[0], new(string)).(*string)

	return out0, err

}

// DEFAULTPORTID is a free data retrieval call binding the contract method 0x78e8e9fb.
//
// Solidity: function DEFAULT_PORT_ID() view returns(string)
func (_Lib *LibSession) DEFAULTPORTID() (string, error) {
	return _Lib.Contract.DEFAULTPORTID(&_Lib.CallOpts)
}

// DEFAULTPORTID is a free data retrieval call binding the contract method 0x78e8e9fb.
//
// Solidity: function DEFAULT_PORT_ID() view returns(string)
func (_Lib *LibCallerSession) DEFAULTPORTID() (string, error) {
	return _Lib.Contract.DEFAULTPORTID(&_Lib.CallOpts)
}

// FAILEDACKNOWLEDGEMENTJSON is a free data retrieval call binding the contract method 0xf171d714.
//
// Solidity: function FAILED_ACKNOWLEDGEMENT_JSON() view returns(bytes)
func (_Lib *LibCaller) FAILEDACKNOWLEDGEMENTJSON(opts *bind.CallOpts) ([]byte, error) {
	var out []interface{}
	err := _Lib.contract.Call(opts, &out, "FAILED_ACKNOWLEDGEMENT_JSON")

	if err != nil {
		return *new([]byte), err
	}

	out0 := *abi.ConvertType(out[0], new([]byte)).(*[]byte)

	return out0, err

}

// FAILEDACKNOWLEDGEMENTJSON is a free data retrieval call binding the contract method 0xf171d714.
//
// Solidity: function FAILED_ACKNOWLEDGEMENT_JSON() view returns(bytes)
func (_Lib *LibSession) FAILEDACKNOWLEDGEMENTJSON() ([]byte, error) {
	return _Lib.Contract.FAILEDACKNOWLEDGEMENTJSON(&_Lib.CallOpts)
}

// FAILEDACKNOWLEDGEMENTJSON is a free data retrieval call binding the contract method 0xf171d714.
//
// Solidity: function FAILED_ACKNOWLEDGEMENT_JSON() view returns(bytes)
func (_Lib *LibCallerSession) FAILEDACKNOWLEDGEMENTJSON() ([]byte, error) {
	return _Lib.Contract.FAILEDACKNOWLEDGEMENTJSON(&_Lib.CallOpts)
}

// ICS20ENCODING is a free data retrieval call binding the contract method 0x00e98b3b.
//
// Solidity: function ICS20_ENCODING() view returns(string)
func (_Lib *LibCaller) ICS20ENCODING(opts *bind.CallOpts) (string, error) {
	var out []interface{}
	err := _Lib.contract.Call(opts, &out, "ICS20_ENCODING")

	if err != nil {
		return *new(string), err
	}

	out0 := *abi.ConvertType(out[0], new(string)).(*string)

	return out0, err

}

// ICS20ENCODING is a free data retrieval call binding the contract method 0x00e98b3b.
//
// Solidity: function ICS20_ENCODING() view returns(string)
func (_Lib *LibSession) ICS20ENCODING() (string, error) {
	return _Lib.Contract.ICS20ENCODING(&_Lib.CallOpts)
}

// ICS20ENCODING is a free data retrieval call binding the contract method 0x00e98b3b.
//
// Solidity: function ICS20_ENCODING() view returns(string)
func (_Lib *LibCallerSession) ICS20ENCODING() (string, error) {
	return _Lib.Contract.ICS20ENCODING(&_Lib.CallOpts)
}

// ICS20VERSION is a free data retrieval call binding the contract method 0x025183eb.
//
// Solidity: function ICS20_VERSION() view returns(string)
func (_Lib *LibCaller) ICS20VERSION(opts *bind.CallOpts) (string, error) {
	var out []interface{}
	err := _Lib.contract.Call(opts, &out, "ICS20_VERSION")

	if err != nil {
		return *new(string), err
	}

	out0 := *abi.ConvertType(out[0], new(string)).(*string)

	return out0, err

}

// ICS20VERSION is a free data retrieval call binding the contract method 0x025183eb.
//
// Solidity: function ICS20_VERSION() view returns(string)
func (_Lib *LibSession) ICS20VERSION() (string, error) {
	return _Lib.Contract.ICS20VERSION(&_Lib.CallOpts)
}

// ICS20VERSION is a free data retrieval call binding the contract method 0x025183eb.
//
// Solidity: function ICS20_VERSION() view returns(string)
func (_Lib *LibCallerSession) ICS20VERSION() (string, error) {
	return _Lib.Contract.ICS20VERSION(&_Lib.CallOpts)
}

// SUCCESSFULACKNOWLEDGEMENTJSON is a free data retrieval call binding the contract method 0x96ce842b.
//
// Solidity: function SUCCESSFUL_ACKNOWLEDGEMENT_JSON() view returns(bytes)
func (_Lib *LibCaller) SUCCESSFULACKNOWLEDGEMENTJSON(opts *bind.CallOpts) ([]byte, error) {
	var out []interface{}
	err := _Lib.contract.Call(opts, &out, "SUCCESSFUL_ACKNOWLEDGEMENT_JSON")

	if err != nil {
		return *new([]byte), err
	}

	out0 := *abi.ConvertType(out[0], new([]byte)).(*[]byte)

	return out0, err

}

// SUCCESSFULACKNOWLEDGEMENTJSON is a free data retrieval call binding the contract method 0x96ce842b.
//
// Solidity: function SUCCESSFUL_ACKNOWLEDGEMENT_JSON() view returns(bytes)
func (_Lib *LibSession) SUCCESSFULACKNOWLEDGEMENTJSON() ([]byte, error) {
	return _Lib.Contract.SUCCESSFULACKNOWLEDGEMENTJSON(&_Lib.CallOpts)
}

// SUCCESSFULACKNOWLEDGEMENTJSON is a free data retrieval call binding the contract method 0x96ce842b.
//
// Solidity: function SUCCESSFUL_ACKNOWLEDGEMENT_JSON() view returns(bytes)
func (_Lib *LibCallerSession) SUCCESSFULACKNOWLEDGEMENTJSON() ([]byte, error) {
	return _Lib.Contract.SUCCESSFULACKNOWLEDGEMENTJSON(&_Lib.CallOpts)
}

// AbiPublicTypes is a free data retrieval call binding the contract method 0xf5bfab69.
//
// Solidity: function abiPublicTypes((((string,(string,string)[]),uint256)[],string,string,string,(string,(string,string)[])) o1) pure returns()
func (_Lib *LibCaller) AbiPublicTypes(opts *bind.CallOpts, o1 ICS20LibFungibleTokenPacketDataV2) error {
	var out []interface{}
	err := _Lib.contract.Call(opts, &out, "abiPublicTypes", o1)

	if err != nil {
		return err
	}

	return err

}

// AbiPublicTypes is a free data retrieval call binding the contract method 0xf5bfab69.
//
// Solidity: function abiPublicTypes((((string,(string,string)[]),uint256)[],string,string,string,(string,(string,string)[])) o1) pure returns()
func (_Lib *LibSession) AbiPublicTypes(o1 ICS20LibFungibleTokenPacketDataV2) error {
	return _Lib.Contract.AbiPublicTypes(&_Lib.CallOpts, o1)
}

// AbiPublicTypes is a free data retrieval call binding the contract method 0xf5bfab69.
//
// Solidity: function abiPublicTypes((((string,(string,string)[]),uint256)[],string,string,string,(string,(string,string)[])) o1) pure returns()
func (_Lib *LibCallerSession) AbiPublicTypes(o1 ICS20LibFungibleTokenPacketDataV2) error {
	return _Lib.Contract.AbiPublicTypes(&_Lib.CallOpts, o1)
}

// GetPath is a free data retrieval call binding the contract method 0xa472ba28.
//
// Solidity: function getPath((string,(string,string)[]) denom) pure returns(string)
func (_Lib *LibCaller) GetPath(opts *bind.CallOpts, denom ICS20LibDenom) (string, error) {
	var out []interface{}
	err := _Lib.contract.Call(opts, &out, "getPath", denom)

	if err != nil {
		return *new(string), err
	}

	out0 := *abi.ConvertType(out[0], new(string)).(*string)

	return out0, err

}

// GetPath is a free data retrieval call binding the contract method 0xa472ba28.
//
// Solidity: function getPath((string,(string,string)[]) denom) pure returns(string)
func (_Lib *LibSession) GetPath(denom ICS20LibDenom) (string, error) {
	return _Lib.Contract.GetPath(&_Lib.CallOpts, denom)
}

// GetPath is a free data retrieval call binding the contract method 0xa472ba28.
//
// Solidity: function getPath((string,(string,string)[]) denom) pure returns(string)
func (_Lib *LibCallerSession) GetPath(denom ICS20LibDenom) (string, error) {
	return _Lib.Contract.GetPath(&_Lib.CallOpts, denom)
}

// NewMsgSendPacketV2 is a free data retrieval call binding the contract method 0x27340635.
//
// Solidity: function newMsgSendPacketV2(address sender, ((address,uint256)[],string,string,string,uint64,string,((string,string)[])) msg_) view returns((string,uint64,(string,string,string,string,bytes)[]))
func (_Lib *LibCaller) NewMsgSendPacketV2(opts *bind.CallOpts, sender common.Address, msg_ IICS20TransferMsgsSendTransferMsg) (IICS26RouterMsgsMsgSendPacket, error) {
	var out []interface{}
	err := _Lib.contract.Call(opts, &out, "newMsgSendPacketV2", sender, msg_)

	if err != nil {
		return *new(IICS26RouterMsgsMsgSendPacket), err
	}

	out0 := *abi.ConvertType(out[0], new(IICS26RouterMsgsMsgSendPacket)).(*IICS26RouterMsgsMsgSendPacket)

	return out0, err

}

// NewMsgSendPacketV2 is a free data retrieval call binding the contract method 0x27340635.
//
// Solidity: function newMsgSendPacketV2(address sender, ((address,uint256)[],string,string,string,uint64,string,((string,string)[])) msg_) view returns((string,uint64,(string,string,string,string,bytes)[]))
func (_Lib *LibSession) NewMsgSendPacketV2(sender common.Address, msg_ IICS20TransferMsgsSendTransferMsg) (IICS26RouterMsgsMsgSendPacket, error) {
	return _Lib.Contract.NewMsgSendPacketV2(&_Lib.CallOpts, sender, msg_)
}

// NewMsgSendPacketV2 is a free data retrieval call binding the contract method 0x27340635.
//
// Solidity: function newMsgSendPacketV2(address sender, ((address,uint256)[],string,string,string,uint64,string,((string,string)[])) msg_) view returns((string,uint64,(string,string,string,string,bytes)[]))
func (_Lib *LibCallerSession) NewMsgSendPacketV2(sender common.Address, msg_ IICS20TransferMsgsSendTransferMsg) (IICS26RouterMsgsMsgSendPacket, error) {
	return _Lib.Contract.NewMsgSendPacketV2(&_Lib.CallOpts, sender, msg_)
}
