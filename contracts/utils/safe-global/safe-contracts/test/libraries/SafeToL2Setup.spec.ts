import { expect } from "chai";
import hre, { deployments, ethers } from "hardhat";
import { getFactory, getSafeL2SingletonContract, getSafeSingletonContract, getSafeWithOwners } from "../utils/setup";
import { sameHexString } from "../utils/strings";
import { executeContractCallWithSigners } from "../../src";
import { EXPECTED_SAFE_STORAGE_LAYOUT, getContractStorageLayout } from "../utils/storage";

type HardhatTraceLog = {
    depth: number;
    gas: number;
    gasCost: number;
    op: string;
    pc: number;
    stack: string[];
    storage: { [key: string]: string };
    memory: string;
};

type HardhatTrace = {
    failed: boolean;
    gas: number;
    returnValue: string;
    structLogs: HardhatTraceLog[];
};

describe("SafeToL2Setup", () => {
    const setupTests = deployments.createFixture(async ({ deployments }) => {
        await deployments.fixture();
        const safeToL2SetupLib = await (await hre.ethers.getContractFactory("SafeToL2Setup")).deploy();
        const signers = await ethers.getSigners();
        const safeSingleton = await getSafeSingletonContract();
        const safeL2 = await getSafeL2SingletonContract();
        const proxyFactory = await getFactory();
        return {
            safeToL2SetupLib,
            signers,
            safeSingleton,
            safeL2,
            proxyFactory,
        };
    });

    describe("L2", () => {
        before(function () {
            if (hre.network.config.chainId === 1) {
                this.skip();
            }
        });

        describe("setupToL2", () => {
            it("follows the expected storage layout", async () => {
                const safeStorageLayout = await getContractStorageLayout(hre, "SafeToL2Setup");

                expect(safeStorageLayout).to.deep.eq(EXPECTED_SAFE_STORAGE_LAYOUT);
            });

            it("should emit an event", async () => {
                const {
                    safeSingleton,
                    safeL2,
                    proxyFactory,
                    signers: [user1],
                    safeToL2SetupLib,
                } = await setupTests();
                const safeL2SingletonAddress = safeL2.address;
                const safeToL2SetupCall = safeToL2SetupLib.interface.encodeFunctionData("setupToL2", [safeL2SingletonAddress]);

                const setupData = safeL2.interface.encodeFunctionData("setup", [
                    [user1.address],
                    1,
                    safeToL2SetupLib.address,
                    safeToL2SetupCall,
                    ethers.constants.AddressZero,
                    ethers.constants.AddressZero,
                    0,
                    ethers.constants.AddressZero,
                ]);
                const safeAddress = await proxyFactory.callStatic.createProxyWithNonce(safeSingleton.address, setupData, 0);

                await expect(proxyFactory.createProxyWithNonce(safeSingleton.address, setupData, 0))
                    .to.emit(safeToL2SetupLib.attach(safeAddress), "ChangedMasterCopy")
                    .withArgs(safeL2SingletonAddress);
            });

            it("only allows singleton address that contains code", async () => {
                const {
                    safeSingleton,
                    safeL2,
                    proxyFactory,
                    signers: [user1, user2],
                    safeToL2SetupLib,
                } = await setupTests();
                const safeToL2SetupCall = safeToL2SetupLib.interface.encodeFunctionData("setupToL2", [user2.address]);

                const setupData = safeL2.interface.encodeFunctionData("setup", [
                    [user1.address],
                    1,
                    safeToL2SetupLib.address,
                    safeToL2SetupCall,
                    ethers.constants.AddressZero,
                    ethers.constants.AddressZero,
                    0,
                    ethers.constants.AddressZero,
                ]);

                // For some reason, hardhat can't infer the revert reason
                await expect(proxyFactory.createProxyWithNonce(safeSingleton.address, setupData, 0)).to.be.reverted;
            });

            it("can be used only via DELEGATECALL opcode", async () => {
                const { safeToL2SetupLib } = await setupTests();
                const randomAddress = ethers.utils.hexlify(ethers.utils.randomBytes(20));

                await expect(safeToL2SetupLib.setupToL2(randomAddress)).to.be.revertedWith(
                    "SafeToL2Setup should only be called via delegatecall",
                );
            });

            it("can only be used through Safe initialization process", async () => {
                const {
                    safeToL2SetupLib,
                    signers: [user1],
                } = await setupTests();
                const safe = await getSafeWithOwners([user1.address]);
                const safeToL2SetupLibAddress = safeToL2SetupLib.address;

                await expect(
                    executeContractCallWithSigners(safe, safeToL2SetupLib, "setupToL2", [safeToL2SetupLibAddress], [user1], true),
                ).to.be.revertedWith("GS013");
            });

            it("changes the expected storage slot without touching the most important ones", async () => {
                const {
                    safeSingleton,
                    safeL2,
                    proxyFactory,
                    signers: [user1],
                    safeToL2SetupLib,
                } = await setupTests();

                const safeL2SingletonAddress = safeL2.address;
                const safeToL2SetupLibAddress = safeToL2SetupLib.address;
                const safeToL2SetupCall = safeToL2SetupLib.interface.encodeFunctionData("setupToL2", [safeL2SingletonAddress]);

                const setupData = safeL2.interface.encodeFunctionData("setup", [
                    [user1.address],
                    1,
                    safeToL2SetupLib.address,
                    safeToL2SetupCall,
                    ethers.constants.AddressZero,
                    ethers.constants.AddressZero,
                    0,
                    ethers.constants.AddressZero,
                ]);
                const safeAddress = await proxyFactory.callStatic.createProxyWithNonce(safeSingleton.address, setupData, 0);

                const transaction = await (await proxyFactory.createProxyWithNonce(safeSingleton.address, setupData, 0)).wait();
                if (!transaction?.transactionHash) {
                    throw new Error("No transaction hash");
                }
                // I decided to use tracing for this test because it gives an overview of all the storage slots involved in the transaction
                // Alternatively, one could use `eth_getStorageAt` to check storage slots directly
                // But that would not guarantee that other storage slots were not touched during the transaction
                const trace = (await hre.network.provider.send("debug_traceTransaction", [transaction.transactionHash])) as HardhatTrace;
                // Hardhat uses the most basic struct/opcode logger tracer: https://geth.ethereum.org/docs/developers/evm-tracing/built-in-tracers#struct-opcode-logger
                // To find the "snapshot" of the storage before the DELEGATECALL into the library, we need to find the first DELEGATECALL opcode calling into the library
                // To do that, we search for the DELEGATECALL opcode with the stack input pointing to the library address
                const delegateCallIntoTheLib = trace.structLogs.findIndex(
                    (log) =>
                        log.op === "DELEGATECALL" &&
                        sameHexString(
                            log.stack[log.stack.length - 2],
                            ethers.utils.hexlify(ethers.utils.zeroPad(safeToL2SetupLibAddress, 32)).slice(2),
                        ),
                );
                const preDelegateCallStorage = trace.structLogs[delegateCallIntoTheLib].storage;

                // The SafeSetup event is emitted after the Safe is set up
                // To get the storage snapshot after the Safe is set up, we need to find the LOG2 opcode with the topic input on the stack equal the SafeSetup event signature
                const SAFE_SETUP_EVENT_SIGNATURE = ethers.utils.keccak256(
                    ethers.utils.toUtf8Bytes(safeSingleton.interface.getEvent("SafeSetup").format("sighash")),
                );
                const postSafeSetup = trace.structLogs.find(
                    (log, index) =>
                        log.op === "LOG2" &&
                        log.stack[log.stack.length - 3] === SAFE_SETUP_EVENT_SIGNATURE.slice(2) &&
                        index > delegateCallIntoTheLib,
                );
                if (!postSafeSetup) {
                    throw new Error("No SafeSetup event");
                }
                const postSafeSetupStorage = postSafeSetup.storage;

                for (const [key, value] of Object.entries(postSafeSetupStorage)) {
                    // The slot key 0 is the singleton storage slot, it must equal the L2 singleton address
                    if (sameHexString(key, ethers.utils.hexlify(ethers.utils.zeroPad("0x00", 32)))) {
                        expect(sameHexString(ethers.utils.hexlify(ethers.utils.zeroPad(safeL2SingletonAddress, 32)), value)).to.be.true;
                    } else {
                        // All other storage slots must be the same as before the DELEGATECALL
                        if (key in preDelegateCallStorage) {
                            expect(sameHexString(preDelegateCallStorage[key], value)).to.be.true;
                        } else {
                            // This special case is needed because the SafeToL2Setup library inherits the SafeStorage library
                            // And that makes the tracer report all the storage slots in the SafeStorage library as well
                            // Even though if they were not touched during the transaction
                            expect(sameHexString(value, "0".repeat(64))).to.be.true;
                        }
                    }
                }

                // Double-check that the storage slot was changed at the end of the transaction
                const singletonInStorage = await hre.ethers.provider.getStorageAt(safeAddress, ethers.utils.zeroPad("0x00", 32));
                expect(sameHexString(singletonInStorage, ethers.utils.hexlify(ethers.utils.zeroPad(safeL2SingletonAddress, 32)))).to.be
                    .true;
            });
        });
    });

    describe("L1", () => {
        before(function () {
            if (hre.network.config.chainId !== 1) {
                this.skip();
            }
        });

        it("should be a noop when the chain id is 1", async () => {
            const {
                safeSingleton,
                safeL2,
                proxyFactory,
                signers: [user1],
                safeToL2SetupLib,
            } = await setupTests();
            const safeSingeltonAddress = safeSingleton.address;
            const safeL2SingletonAddress = safeL2.address;
            const safeToL2SetupCall = safeToL2SetupLib.interface.encodeFunctionData("setupToL2", [safeL2SingletonAddress]);

            const setupData = safeL2.interface.encodeFunctionData("setup", [
                [user1.address],
                1,
                safeToL2SetupLib.address,
                safeToL2SetupCall,
                ethers.constants.AddressZero,
                ethers.constants.AddressZero,
                0,
                ethers.constants.AddressZero,
            ]);
            const safeAddress = await proxyFactory.callStatic.createProxyWithNonce(safeSingleton.address, setupData, 0);

            await expect(proxyFactory.createProxyWithNonce(safeSingeltonAddress, setupData, 0)).to.not.emit(
                safeToL2SetupLib.attach(safeAddress),
                "ChangedMasterCopy",
            );
            const singletonInStorage = await hre.ethers.provider.getStorageAt(safeAddress, ethers.utils.zeroPad("0x00", 32));
            expect(sameHexString(singletonInStorage, ethers.utils.hexlify(ethers.utils.zeroPad(safeSingeltonAddress, 32)))).to.be.true;
        });
    });
});
