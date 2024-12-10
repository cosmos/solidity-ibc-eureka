"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.buildSafeTransaction = exports.executeContractCallWithSigners = exports.executeTxWithSigners = exports.buildContractCall = exports.populateExecuteTx = exports.executeTx = exports.logGas = exports.buildSignatureBytes = exports.buildContractSignature = exports.safeSignMessage = exports.signHash = exports.safeSignTypedData = exports.safeApproveHash = exports.calculateSafeMessageHash = exports.preimageSafeMessageHash = exports.calculateSafeTransactionHash = exports.preimageSafeTransactionHash = exports.calculateSafeDomainSeparator = exports.EIP712_SAFE_MESSAGE_TYPE = exports.EIP712_SAFE_TX_TYPE = exports.EIP_DOMAIN = void 0;
const ethers_1 = require("ethers");
const constants_1 = require("@ethersproject/constants");
exports.EIP_DOMAIN = {
    EIP712Domain: [
        { type: "uint256", name: "chainId" },
        { type: "address", name: "verifyingContract" },
    ],
};
exports.EIP712_SAFE_TX_TYPE = {
    // "SafeTx(address to,uint256 value,bytes data,uint8 operation,uint256 safeTxGas,uint256 baseGas,uint256 gasPrice,address gasToken,address refundReceiver,uint256 nonce)"
    SafeTx: [
        { type: "address", name: "to" },
        { type: "uint256", name: "value" },
        { type: "bytes", name: "data" },
        { type: "uint8", name: "operation" },
        { type: "uint256", name: "safeTxGas" },
        { type: "uint256", name: "baseGas" },
        { type: "uint256", name: "gasPrice" },
        { type: "address", name: "gasToken" },
        { type: "address", name: "refundReceiver" },
        { type: "uint256", name: "nonce" },
    ],
};
exports.EIP712_SAFE_MESSAGE_TYPE = {
    // "SafeMessage(bytes message)"
    SafeMessage: [{ type: "bytes", name: "message" }],
};
const calculateSafeDomainSeparator = (safe, chainId) => {
    return ethers_1.utils._TypedDataEncoder.hashDomain({ verifyingContract: safe.address, chainId });
};
exports.calculateSafeDomainSeparator = calculateSafeDomainSeparator;
const preimageSafeTransactionHash = (safe, safeTx, chainId) => {
    return ethers_1.utils._TypedDataEncoder.encode({ verifyingContract: safe.address, chainId }, exports.EIP712_SAFE_TX_TYPE, safeTx);
};
exports.preimageSafeTransactionHash = preimageSafeTransactionHash;
const calculateSafeTransactionHash = (safe, safeTx, chainId) => {
    return ethers_1.utils._TypedDataEncoder.hash({ verifyingContract: safe.address, chainId }, exports.EIP712_SAFE_TX_TYPE, safeTx);
};
exports.calculateSafeTransactionHash = calculateSafeTransactionHash;
const preimageSafeMessageHash = (safe, message, chainId) => {
    return ethers_1.utils._TypedDataEncoder.encode({ verifyingContract: safe.address, chainId }, exports.EIP712_SAFE_MESSAGE_TYPE, { message });
};
exports.preimageSafeMessageHash = preimageSafeMessageHash;
const calculateSafeMessageHash = (safe, message, chainId) => {
    return ethers_1.utils._TypedDataEncoder.hash({ verifyingContract: safe.address, chainId }, exports.EIP712_SAFE_MESSAGE_TYPE, { message });
};
exports.calculateSafeMessageHash = calculateSafeMessageHash;
const safeApproveHash = async (signer, safe, safeTx, skipOnChainApproval) => {
    if (!skipOnChainApproval) {
        if (!signer.provider)
            throw Error("Provider required for on-chain approval");
        const chainId = (await signer.provider.getNetwork()).chainId;
        const typedDataHash = ethers_1.utils.arrayify((0, exports.calculateSafeTransactionHash)(safe, safeTx, chainId));
        const signerSafe = safe.connect(signer);
        await signerSafe.approveHash(typedDataHash);
    }
    const signerAddress = await signer.getAddress();
    return {
        signer: signerAddress,
        data: "0x000000000000000000000000" +
            signerAddress.slice(2) +
            "0000000000000000000000000000000000000000000000000000000000000000" +
            "01",
    };
};
exports.safeApproveHash = safeApproveHash;
const safeSignTypedData = async (signer, safe, safeTx, chainId) => {
    if (!chainId && !signer.provider)
        throw Error("Provider required to retrieve chainId");
    const cid = chainId || (await signer.provider.getNetwork()).chainId;
    const signerAddress = await signer.getAddress();
    return {
        signer: signerAddress,
        data: await signer._signTypedData({ verifyingContract: safe.address, chainId: cid }, exports.EIP712_SAFE_TX_TYPE, safeTx),
    };
};
exports.safeSignTypedData = safeSignTypedData;
const signHash = async (signer, hash) => {
    const typedDataHash = ethers_1.utils.arrayify(hash);
    const signerAddress = await signer.getAddress();
    return {
        signer: signerAddress,
        data: (await signer.signMessage(typedDataHash)).replace(/1b$/, "1f").replace(/1c$/, "20"),
    };
};
exports.signHash = signHash;
const safeSignMessage = async (signer, safe, safeTx, chainId) => {
    const cid = chainId || (await signer.provider.getNetwork()).chainId;
    return (0, exports.signHash)(signer, (0, exports.calculateSafeTransactionHash)(safe, safeTx, cid));
};
exports.safeSignMessage = safeSignMessage;
const buildContractSignature = (signerAddress, signature) => {
    return {
        signer: signerAddress,
        data: signature,
        dynamic: true,
    };
};
exports.buildContractSignature = buildContractSignature;
const buildSignatureBytes = (signatures) => {
    const SIGNATURE_LENGTH_BYTES = 65;
    signatures.sort((left, right) => left.signer.toLowerCase().localeCompare(right.signer.toLowerCase()));
    let signatureBytes = "0x";
    let dynamicBytes = "";
    for (const sig of signatures) {
        if (sig.dynamic) {
            /*
                A contract signature has a static part of 65 bytes and the dynamic part that needs to be appended at the end of
                end signature bytes.
                The signature format is
                Signature type == 0
                Constant part: 65 bytes
                {32-bytes signature verifier}{32-bytes dynamic data position}{1-byte signature type}
                Dynamic part (solidity bytes): 32 bytes + signature data length
                {32-bytes signature length}{bytes signature data}
            */
            const dynamicPartPosition = (signatures.length * SIGNATURE_LENGTH_BYTES + dynamicBytes.length / 2)
                .toString(16)
                .padStart(64, "0");
            const dynamicPartLength = (sig.data.slice(2).length / 2).toString(16).padStart(64, "0");
            const staticSignature = `${sig.signer.slice(2).padStart(64, "0")}${dynamicPartPosition}00`;
            const dynamicPartWithLength = `${dynamicPartLength}${sig.data.slice(2)}`;
            signatureBytes += staticSignature;
            dynamicBytes += dynamicPartWithLength;
        }
        else {
            signatureBytes += sig.data.slice(2);
        }
    }
    return signatureBytes + dynamicBytes;
};
exports.buildSignatureBytes = buildSignatureBytes;
const logGas = async (message, tx, skip) => {
    return tx.then(async (result) => {
        const receipt = await result.wait();
        if (!skip)
            console.log("           Used", receipt.gasUsed.toNumber(), `gas for >${message}<`);
        return result;
    });
};
exports.logGas = logGas;
const executeTx = async (safe, safeTx, signatures, overrides) => {
    const signatureBytes = (0, exports.buildSignatureBytes)(signatures);
    return safe.execTransaction(safeTx.to, safeTx.value, safeTx.data, safeTx.operation, safeTx.safeTxGas, safeTx.baseGas, safeTx.gasPrice, safeTx.gasToken, safeTx.refundReceiver, signatureBytes, overrides || {});
};
exports.executeTx = executeTx;
const populateExecuteTx = async (safe, safeTx, signatures, overrides) => {
    const signatureBytes = (0, exports.buildSignatureBytes)(signatures);
    return safe.populateTransaction.execTransaction(safeTx.to, safeTx.value, safeTx.data, safeTx.operation, safeTx.safeTxGas, safeTx.baseGas, safeTx.gasPrice, safeTx.gasToken, safeTx.refundReceiver, signatureBytes, overrides || {});
};
exports.populateExecuteTx = populateExecuteTx;
const buildContractCall = (contract, method, params, nonce, delegateCall, overrides) => {
    const data = contract.interface.encodeFunctionData(method, params);
    return (0, exports.buildSafeTransaction)(Object.assign({
        to: contract.address,
        data,
        operation: delegateCall ? 1 : 0,
        nonce,
    }, overrides));
};
exports.buildContractCall = buildContractCall;
const executeTxWithSigners = async (safe, tx, signers, overrides) => {
    const sigs = await Promise.all(signers.map((signer) => (0, exports.safeSignTypedData)(signer, safe, tx)));
    return (0, exports.executeTx)(safe, tx, sigs, overrides);
};
exports.executeTxWithSigners = executeTxWithSigners;
const executeContractCallWithSigners = async (safe, contract, method, params, signers, delegateCall, overrides) => {
    const tx = (0, exports.buildContractCall)(contract, method, params, await safe.nonce(), delegateCall, overrides);
    return (0, exports.executeTxWithSigners)(safe, tx, signers);
};
exports.executeContractCallWithSigners = executeContractCallWithSigners;
const buildSafeTransaction = (template) => {
    return {
        to: template.to,
        value: template.value || 0,
        data: template.data || "0x",
        operation: template.operation || 0,
        safeTxGas: template.safeTxGas || 0,
        baseGas: template.baseGas || 0,
        gasPrice: template.gasPrice || 0,
        gasToken: template.gasToken || constants_1.AddressZero,
        refundReceiver: template.refundReceiver || constants_1.AddressZero,
        nonce: template.nonce,
    };
};
exports.buildSafeTransaction = buildSafeTransaction;
