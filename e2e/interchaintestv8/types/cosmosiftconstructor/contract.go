// Code generated - DO NOT EDIT.
// This file is a generated binding and any manual changes will be lost.

package cosmosiftconstructor

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

// ContractMetaData contains all meta data concerning the Contract contract.
var ContractMetaData = &bind.MetaData{
	ABI: "[{\"type\":\"constructor\",\"inputs\":[{\"name\":\"bridgeReceiveTypeUrl_\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"denom_\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"icaAddress_\",\"type\":\"string\",\"internalType\":\"string\"}],\"stateMutability\":\"nonpayable\"},{\"type\":\"function\",\"name\":\"bridgeReceiveTypeUrl\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"string\",\"internalType\":\"string\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"constructMintCall\",\"inputs\":[{\"name\":\"receiver\",\"type\":\"string\",\"internalType\":\"string\"},{\"name\":\"amount\",\"type\":\"uint256\",\"internalType\":\"uint256\"}],\"outputs\":[{\"name\":\"\",\"type\":\"bytes\",\"internalType\":\"bytes\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"denom\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"string\",\"internalType\":\"string\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"icaAddress\",\"inputs\":[],\"outputs\":[{\"name\":\"\",\"type\":\"string\",\"internalType\":\"string\"}],\"stateMutability\":\"view\"},{\"type\":\"function\",\"name\":\"supportsInterface\",\"inputs\":[{\"name\":\"interfaceId\",\"type\":\"bytes4\",\"internalType\":\"bytes4\"}],\"outputs\":[{\"name\":\"\",\"type\":\"bool\",\"internalType\":\"bool\"}],\"stateMutability\":\"view\"}]",
	Bin: "0x60806040523461046857610f95803803806100198161046c565b92833981016060828203126104685781516001600160401b0381116104685781610044918401610491565b60208301519092906001600160401b0381116104685782610066918301610491565b60408201519092906001600160401b038111610468576100869201610491565b82516001600160401b0381116102a3575f54600181811c9116801561045e575b602082101461028557601f81116103fc575b506020601f821160011461039b57819293945f92610390575b50508160011b915f199060031b1c1916175f555b81516001600160401b0381116102a357600154600181811c91168015610386575b602082101461028557601f8111610323575b50602092601f82116001146102c257928192935f926102b7575b50508160011b915f199060031b1c1916176001555b80516001600160401b0381116102a357600254600181811c91168015610299575b602082101461028557601f8111610222575b50602091601f82116001146101c2579181925f926101b7575b50508160011b915f199060031b1c1916176002555b604051610ab290816104e38239f35b015190505f80610193565b601f1982169260025f52805f20915f5b85811061020a575083600195106101f2575b505050811b016002556101a8565b01515f1960f88460031b161c191690555f80806101e4565b919260206001819286850151815501940192016101d2565b60025f527f405787fa12a823e0f2b7631cc41b3ba8828b3321ca811111fa75cd3aa3bb5ace601f830160051c8101916020841061027b575b601f0160051c01905b818110610270575061017a565b5f8155600101610263565b909150819061025a565b634e487b7160e01b5f52602260045260245ffd5b90607f1690610168565b634e487b7160e01b5f52604160045260245ffd5b015190505f80610132565b601f1982169360015f52805f20915f5b86811061030b57508360019596106102f3575b505050811b01600155610147565b01515f1960f88460031b161c191690555f80806102e5565b919260206001819286850151815501940192016102d2565b60015f527fb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6601f830160051c8101916020841061037c575b601f0160051c01905b8181106103715750610118565b5f8155600101610364565b909150819061035b565b90607f1690610106565b015190505f806100d1565b601f198216905f8052805f20915f5b8181106103e4575095836001959697106103cc575b505050811b015f556100e5565b01515f1960f88460031b161c191690555f80806103bf565b9192602060018192868b0151815501940192016103aa565b5f80527f290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563601f830160051c81019160208410610454575b601f0160051c01905b81811061044957506100b8565b5f815560010161043c565b9091508190610433565b90607f16906100a6565b5f80fd5b6040519190601f01601f191682016001600160401b038111838210176102a357604052565b81601f82011215610468578051906001600160401b0382116102a3576104c0601f8301601f191660200161046c565b928284526020838301011161046857815f9260208093018386015e830101529056fe6080806040526004361015610012575f80fd5b5f3560e01c90816301ffc9a7146108ad575080632ff02e27146107ea57806356d981a7146102355780638f2edc84146101755763c370b04214610053575f80fd5b34610171575f7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc360112610171576040515f60015461009181610969565b808452906001811690811561012f57506001146100d1575b6100cd836100b9818503826109ba565b604051918291602083526020830190610a28565b0390f35b60015f9081527fb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6939250905b808210610115575090915081016020016100b96100a9565b9192600181602092548385880101520191019092916100fd565b7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff001660208086019190915291151560051b840190910191506100b990506100a9565b5f80fd5b34610171575f7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc360112610171576040515f5f546101b281610969565b808452906001811690811561012f57506001146101d9576100cd836100b9818503826109ba565b5f8080527f290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563939250905b80821061021b575090915081016020016100b96100a9565b919260018160209254838588010152019101909291610203565b346101715760407ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc3601126101715760043567ffffffffffffffff8111610171573660238201121561017157806004013567ffffffffffffffff8111610171573660248284010111610171575f602435807a184f03e93ff9f4daa797ed6e38ed64bf6a1f0100000000000000008110156107c2575b806d04ee2d6d415b85acef8100000000600a9210156107a7575b662386f26fc10000811015610793575b6305f5e100811015610782575b612710811015610773575b6064811015610765575b101561075b575b6001820190600a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff602161036961035386610a6b565b9561036160405197886109ba565b808752610a6b565b957fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe06020870197013688378501015b01917f30313233343536373839616263646566000000000000000000000000000000008282061a83530480156103f2577fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff600a9192610398565b5050604051927f7b226d65737361676573223a5b7b224074797065223a2200000000000000000060208501525f945f5461042b81610969565b906001811690811561072057506001146106ca575b507f222c227369676e6572223a22000000000000000000000000000000000000000086526002545f9661047282610969565b916001811690811561068c5750600114610633575b50507f222c2264656e6f6d223a2200000000000000000000000000000000000000000086525f956001546104ba81610969565b90600181169081156105f05750600114610592575b5050600e6100b9946004948489600c9660248b977f222c227265636569766572223a220000000000000000000000000000000000006100cd9e52018683013701907f222c22616d6f756e74223a22000000000000000000000000000000000000000084830152518092601a83015e01017f227d5d7d000000000000000000000000000000000000000000000000000000008382015203017fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe48101845201826109ba565b60015f908152919750907fb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf65b8282106105d957505095909501600b0194600e6100b96104cf565b60018160209254600b858d010152019101906105be565b7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0016600b84810191909152821515909202909201019650600e90506100b96104cf565b60025f90815292975090917f405787fa12a823e0f2b7631cc41b3ba8828b3321ca811111fa75cd3aa3bb5ace5b83821061067557505001600c01948680610487565b60018160209254600c858701015201910190610660565b600c9499507fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0091509291921683830152801515020101948680610487565b5f808052919650907f290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e5635b82821061070957505084016037019486610440565b600181602092546037858b010152019101906106f4565b60379398507fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0091501682870152801515028501019486610440565b906001019061031d565b606460029104930192610316565b6127106004910493019261030c565b6305f5e10060089104930192610301565b662386f26fc10000601091049301926102f4565b6d04ee2d6d415b85acef8100000000602091049301926102e4565b50604091507a184f03e93ff9f4daa797ed6e38ed64bf6a1f01000000000000000081046102ca565b34610171575f7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc360112610171576040515f60025461082881610969565b808452906001811690811561012f575060011461084f576100cd836100b9818503826109ba565b60025f9081527f405787fa12a823e0f2b7631cc41b3ba8828b3321ca811111fa75cd3aa3bb5ace939250905b808210610893575090915081016020016100b96100a9565b91926001816020925483858801015201910190929161087b565b346101715760207ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc36011261017157600435907fffffffff00000000000000000000000000000000000000000000000000000000821680920361017157817f56d981a7000000000000000000000000000000000000000000000000000000006020931490811561093f575b5015158152f35b7f01ffc9a70000000000000000000000000000000000000000000000000000000091501483610938565b90600182811c921680156109b0575b602083101461098357565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52602260045260245ffd5b91607f1691610978565b90601f7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0910116810190811067ffffffffffffffff8211176109fb57604052565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52604160045260245ffd5b907fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0601f602080948051918291828752018686015e5f8582860101520116010190565b67ffffffffffffffff81116109fb57601f017fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0166020019056fea164736f6c634300081c000a",
}

// ContractABI is the input ABI used to generate the binding from.
// Deprecated: Use ContractMetaData.ABI instead.
var ContractABI = ContractMetaData.ABI

// ContractBin is the compiled bytecode used for deploying new contracts.
// Deprecated: Use ContractMetaData.Bin instead.
var ContractBin = ContractMetaData.Bin

// DeployContract deploys a new Ethereum contract, binding an instance of Contract to it.
func DeployContract(auth *bind.TransactOpts, backend bind.ContractBackend, bridgeReceiveTypeUrl_ string, denom_ string, icaAddress_ string) (common.Address, *types.Transaction, *Contract, error) {
	parsed, err := ContractMetaData.GetAbi()
	if err != nil {
		return common.Address{}, nil, nil, err
	}
	if parsed == nil {
		return common.Address{}, nil, nil, errors.New("GetABI returned nil")
	}

	address, tx, contract, err := bind.DeployContract(auth, *parsed, common.FromHex(ContractBin), backend, bridgeReceiveTypeUrl_, denom_, icaAddress_)
	if err != nil {
		return common.Address{}, nil, nil, err
	}
	return address, tx, &Contract{ContractCaller: ContractCaller{contract: contract}, ContractTransactor: ContractTransactor{contract: contract}, ContractFilterer: ContractFilterer{contract: contract}}, nil
}

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

// BridgeReceiveTypeUrl is a free data retrieval call binding the contract method 0x8f2edc84.
//
// Solidity: function bridgeReceiveTypeUrl() view returns(string)
func (_Contract *ContractCaller) BridgeReceiveTypeUrl(opts *bind.CallOpts) (string, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "bridgeReceiveTypeUrl")

	if err != nil {
		return *new(string), err
	}

	out0 := *abi.ConvertType(out[0], new(string)).(*string)

	return out0, err

}

// BridgeReceiveTypeUrl is a free data retrieval call binding the contract method 0x8f2edc84.
//
// Solidity: function bridgeReceiveTypeUrl() view returns(string)
func (_Contract *ContractSession) BridgeReceiveTypeUrl() (string, error) {
	return _Contract.Contract.BridgeReceiveTypeUrl(&_Contract.CallOpts)
}

// BridgeReceiveTypeUrl is a free data retrieval call binding the contract method 0x8f2edc84.
//
// Solidity: function bridgeReceiveTypeUrl() view returns(string)
func (_Contract *ContractCallerSession) BridgeReceiveTypeUrl() (string, error) {
	return _Contract.Contract.BridgeReceiveTypeUrl(&_Contract.CallOpts)
}

// ConstructMintCall is a free data retrieval call binding the contract method 0x56d981a7.
//
// Solidity: function constructMintCall(string receiver, uint256 amount) view returns(bytes)
func (_Contract *ContractCaller) ConstructMintCall(opts *bind.CallOpts, receiver string, amount *big.Int) ([]byte, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "constructMintCall", receiver, amount)

	if err != nil {
		return *new([]byte), err
	}

	out0 := *abi.ConvertType(out[0], new([]byte)).(*[]byte)

	return out0, err

}

// ConstructMintCall is a free data retrieval call binding the contract method 0x56d981a7.
//
// Solidity: function constructMintCall(string receiver, uint256 amount) view returns(bytes)
func (_Contract *ContractSession) ConstructMintCall(receiver string, amount *big.Int) ([]byte, error) {
	return _Contract.Contract.ConstructMintCall(&_Contract.CallOpts, receiver, amount)
}

// ConstructMintCall is a free data retrieval call binding the contract method 0x56d981a7.
//
// Solidity: function constructMintCall(string receiver, uint256 amount) view returns(bytes)
func (_Contract *ContractCallerSession) ConstructMintCall(receiver string, amount *big.Int) ([]byte, error) {
	return _Contract.Contract.ConstructMintCall(&_Contract.CallOpts, receiver, amount)
}

// Denom is a free data retrieval call binding the contract method 0xc370b042.
//
// Solidity: function denom() view returns(string)
func (_Contract *ContractCaller) Denom(opts *bind.CallOpts) (string, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "denom")

	if err != nil {
		return *new(string), err
	}

	out0 := *abi.ConvertType(out[0], new(string)).(*string)

	return out0, err

}

// Denom is a free data retrieval call binding the contract method 0xc370b042.
//
// Solidity: function denom() view returns(string)
func (_Contract *ContractSession) Denom() (string, error) {
	return _Contract.Contract.Denom(&_Contract.CallOpts)
}

// Denom is a free data retrieval call binding the contract method 0xc370b042.
//
// Solidity: function denom() view returns(string)
func (_Contract *ContractCallerSession) Denom() (string, error) {
	return _Contract.Contract.Denom(&_Contract.CallOpts)
}

// IcaAddress is a free data retrieval call binding the contract method 0x2ff02e27.
//
// Solidity: function icaAddress() view returns(string)
func (_Contract *ContractCaller) IcaAddress(opts *bind.CallOpts) (string, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "icaAddress")

	if err != nil {
		return *new(string), err
	}

	out0 := *abi.ConvertType(out[0], new(string)).(*string)

	return out0, err

}

// IcaAddress is a free data retrieval call binding the contract method 0x2ff02e27.
//
// Solidity: function icaAddress() view returns(string)
func (_Contract *ContractSession) IcaAddress() (string, error) {
	return _Contract.Contract.IcaAddress(&_Contract.CallOpts)
}

// IcaAddress is a free data retrieval call binding the contract method 0x2ff02e27.
//
// Solidity: function icaAddress() view returns(string)
func (_Contract *ContractCallerSession) IcaAddress() (string, error) {
	return _Contract.Contract.IcaAddress(&_Contract.CallOpts)
}

// SupportsInterface is a free data retrieval call binding the contract method 0x01ffc9a7.
//
// Solidity: function supportsInterface(bytes4 interfaceId) view returns(bool)
func (_Contract *ContractCaller) SupportsInterface(opts *bind.CallOpts, interfaceId [4]byte) (bool, error) {
	var out []interface{}
	err := _Contract.contract.Call(opts, &out, "supportsInterface", interfaceId)

	if err != nil {
		return *new(bool), err
	}

	out0 := *abi.ConvertType(out[0], new(bool)).(*bool)

	return out0, err

}

// SupportsInterface is a free data retrieval call binding the contract method 0x01ffc9a7.
//
// Solidity: function supportsInterface(bytes4 interfaceId) view returns(bool)
func (_Contract *ContractSession) SupportsInterface(interfaceId [4]byte) (bool, error) {
	return _Contract.Contract.SupportsInterface(&_Contract.CallOpts, interfaceId)
}

// SupportsInterface is a free data retrieval call binding the contract method 0x01ffc9a7.
//
// Solidity: function supportsInterface(bytes4 interfaceId) view returns(bool)
func (_Contract *ContractCallerSession) SupportsInterface(interfaceId [4]byte) (bool, error) {
	return _Contract.Contract.SupportsInterface(&_Contract.CallOpts, interfaceId)
}
