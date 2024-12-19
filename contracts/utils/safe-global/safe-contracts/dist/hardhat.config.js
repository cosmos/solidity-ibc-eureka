"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
require("@nomiclabs/hardhat-ethers");
require("@nomiclabs/hardhat-etherscan");
require("@nomiclabs/hardhat-waffle");
require("solidity-coverage");
require("hardhat-deploy");
const dotenv_1 = __importDefault(require("dotenv"));
const yargs_1 = __importDefault(require("yargs"));
const safe_singleton_factory_1 = require("@safe-global/safe-singleton-factory");
const argv = yargs_1.default
    .option("network", {
    type: "string",
    default: "hardhat",
})
    .help(false)
    .version(false).argv;
// Load environment variables.
dotenv_1.default.config();
const { NODE_URL, INFURA_KEY, MNEMONIC, ETHERSCAN_API_KEY, PK, SOLIDITY_VERSION, SOLIDITY_SETTINGS, HARDHAT_CHAIN_ID } = process.env;
const DEFAULT_MNEMONIC = "candy maple cake sugar pudding cream honey rich smooth crumble sweet treat";
const sharedNetworkConfig = {};
if (PK) {
    sharedNetworkConfig.accounts = [PK];
}
else {
    sharedNetworkConfig.accounts = {
        mnemonic: MNEMONIC || DEFAULT_MNEMONIC,
    };
}
if (["mainnet", "rinkeby", "kovan", "goerli", "ropsten", "mumbai", "polygon"].includes(argv.network) && INFURA_KEY === undefined) {
    throw new Error(`Could not find Infura key in env, unable to connect to network ${argv.network}`);
}
require("./src/tasks/local_verify");
require("./src/tasks/deploy_contracts");
require("./src/tasks/show_codesize");
const bignumber_1 = require("@ethersproject/bignumber");
const primarySolidityVersion = SOLIDITY_VERSION || "0.7.6";
const soliditySettings = SOLIDITY_SETTINGS ? JSON.parse(SOLIDITY_SETTINGS) : undefined;
const deterministicDeployment = (network) => {
    const info = (0, safe_singleton_factory_1.getSingletonFactoryInfo)(parseInt(network));
    if (!info) {
        throw new Error(`
        Safe factory not found for network ${network}. You can request a new deployment at https://github.com/safe-global/safe-singleton-factory.
        For more information, see https://github.com/safe-global/safe-contracts#replay-protection-eip-155
      `);
    }
    return {
        factory: info.address,
        deployer: info.signerAddress,
        funding: bignumber_1.BigNumber.from(info.gasLimit).mul(bignumber_1.BigNumber.from(info.gasPrice)).toString(),
        signedTx: info.transaction,
    };
};
const userConfig = {
    paths: {
        artifacts: "build/artifacts",
        cache: "build/cache",
        deploy: "src/deploy",
        sources: "contracts",
    },
    solidity: {
        compilers: [{ version: primarySolidityVersion, settings: soliditySettings }, { version: "0.6.12" }, { version: "0.5.17" }],
    },
    networks: {
        hardhat: {
            allowUnlimitedContractSize: true,
            blockGasLimit: 100000000,
            gas: 100000000,
            chainId: typeof HARDHAT_CHAIN_ID === "string" && !Number.isNaN(parseInt(HARDHAT_CHAIN_ID)) ? parseInt(HARDHAT_CHAIN_ID) : 31337,
        },
        mainnet: Object.assign(Object.assign({}, sharedNetworkConfig), { url: `https://mainnet.infura.io/v3/${INFURA_KEY}` }),
        gnosis: Object.assign(Object.assign({}, sharedNetworkConfig), { url: "https://rpc.gnosischain.com" }),
        ewc: Object.assign(Object.assign({}, sharedNetworkConfig), { url: `https://rpc.energyweb.org` }),
        goerli: Object.assign(Object.assign({}, sharedNetworkConfig), { url: `https://goerli.infura.io/v3/${INFURA_KEY}` }),
        mumbai: Object.assign(Object.assign({}, sharedNetworkConfig), { url: `https://polygon-mumbai.infura.io/v3/${INFURA_KEY}` }),
        polygon: Object.assign(Object.assign({}, sharedNetworkConfig), { url: `https://polygon-mainnet.infura.io/v3/${INFURA_KEY}` }),
        volta: Object.assign(Object.assign({}, sharedNetworkConfig), { url: `https://volta-rpc.energyweb.org` }),
        bsc: Object.assign(Object.assign({}, sharedNetworkConfig), { url: `https://bsc-dataseed.binance.org/` }),
        arbitrum: Object.assign(Object.assign({}, sharedNetworkConfig), { url: `https://arb1.arbitrum.io/rpc` }),
        fantomTestnet: Object.assign(Object.assign({}, sharedNetworkConfig), { url: `https://rpc.testnet.fantom.network/` }),
        avalanche: Object.assign(Object.assign({}, sharedNetworkConfig), { url: `https://api.avax.network/ext/bc/C/rpc` }),
    },
    deterministicDeployment,
    namedAccounts: {
        deployer: 0,
    },
    mocha: {
        timeout: 2000000,
    },
    etherscan: {
        apiKey: ETHERSCAN_API_KEY,
    },
};
if (NODE_URL) {
    userConfig.networks.custom = Object.assign(Object.assign({}, sharedNetworkConfig), { url: NODE_URL });
}
exports.default = userConfig;
