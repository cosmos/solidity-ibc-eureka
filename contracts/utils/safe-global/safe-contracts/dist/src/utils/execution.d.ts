import { Contract, Wallet, BigNumber, BigNumberish, Signer, PopulatedTransaction } from "ethers";
import { TypedDataSigner } from "@ethersproject/abstract-signer";
export declare const EIP_DOMAIN: {
    EIP712Domain: {
        type: string;
        name: string;
    }[];
};
export declare const EIP712_SAFE_TX_TYPE: {
    SafeTx: {
        type: string;
        name: string;
    }[];
};
export declare const EIP712_SAFE_MESSAGE_TYPE: {
    SafeMessage: {
        type: string;
        name: string;
    }[];
};
export interface MetaTransaction {
    to: string;
    value: string | number | BigNumber;
    data: string;
    operation: number;
}
export interface SafeTransaction extends MetaTransaction {
    safeTxGas: string | number;
    baseGas: string | number;
    gasPrice: string | number;
    gasToken: string;
    refundReceiver: string;
    nonce: string | number;
}
export interface SafeSignature {
    signer: string;
    data: string;
    dynamic?: true;
}
export declare const calculateSafeDomainSeparator: (safe: Contract, chainId: BigNumberish) => string;
export declare const preimageSafeTransactionHash: (safe: Contract, safeTx: SafeTransaction, chainId: BigNumberish) => string;
export declare const calculateSafeTransactionHash: (safe: Contract, safeTx: SafeTransaction, chainId: BigNumberish) => string;
export declare const preimageSafeMessageHash: (safe: Contract, message: string, chainId: BigNumberish) => string;
export declare const calculateSafeMessageHash: (safe: Contract, message: string, chainId: BigNumberish) => string;
export declare const safeApproveHash: (signer: Signer, safe: Contract, safeTx: SafeTransaction, skipOnChainApproval?: boolean) => Promise<SafeSignature>;
export declare const safeSignTypedData: (signer: Signer & TypedDataSigner, safe: Contract, safeTx: SafeTransaction, chainId?: BigNumberish) => Promise<SafeSignature>;
export declare const signHash: (signer: Signer, hash: string) => Promise<SafeSignature>;
export declare const safeSignMessage: (signer: Signer, safe: Contract, safeTx: SafeTransaction, chainId?: BigNumberish) => Promise<SafeSignature>;
export declare const buildContractSignature: (signerAddress: string, signature: string) => SafeSignature;
export declare const buildSignatureBytes: (signatures: SafeSignature[]) => string;
export declare const logGas: (message: string, tx: Promise<any>, skip?: boolean) => Promise<any>;
export declare const executeTx: (safe: Contract, safeTx: SafeTransaction, signatures: SafeSignature[], overrides?: any) => Promise<any>;
export declare const populateExecuteTx: (safe: Contract, safeTx: SafeTransaction, signatures: SafeSignature[], overrides?: any) => Promise<PopulatedTransaction>;
export declare const buildContractCall: (contract: Contract, method: string, params: any[], nonce: number, delegateCall?: boolean, overrides?: Partial<SafeTransaction>) => SafeTransaction;
export declare const executeTxWithSigners: (safe: Contract, tx: SafeTransaction, signers: Wallet[], overrides?: any) => Promise<any>;
export declare const executeContractCallWithSigners: (safe: Contract, contract: Contract, method: string, params: any[], signers: Wallet[], delegateCall?: boolean, overrides?: Partial<SafeTransaction>) => Promise<any>;
export declare const buildSafeTransaction: (template: {
    to: string;
    value?: BigNumber | number | string;
    data?: string;
    operation?: number;
    safeTxGas?: number | string;
    baseGas?: number | string;
    gasPrice?: number | string;
    gasToken?: string;
    refundReceiver?: string;
    nonce: number;
}) => SafeTransaction;
