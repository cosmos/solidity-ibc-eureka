"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.buildMultiSendSafeTx = exports.encodeMultiSend = void 0;
const ethers_1 = require("ethers");
const execution_1 = require("./execution");
const encodeMetaTransaction = (tx) => {
    const data = ethers_1.utils.arrayify(tx.data);
    const encoded = ethers_1.utils.solidityPack(["uint8", "address", "uint256", "uint256", "bytes"], [tx.operation, tx.to, tx.value, data.length, data]);
    return encoded.slice(2);
};
const encodeMultiSend = (txs) => {
    return "0x" + txs.map((tx) => encodeMetaTransaction(tx)).join("");
};
exports.encodeMultiSend = encodeMultiSend;
const buildMultiSendSafeTx = (multiSend, txs, nonce, overrides) => {
    return (0, execution_1.buildContractCall)(multiSend, "multiSend", [(0, exports.encodeMultiSend)(txs)], nonce, true, overrides);
};
exports.buildMultiSendSafeTx = buildMultiSendSafeTx;
