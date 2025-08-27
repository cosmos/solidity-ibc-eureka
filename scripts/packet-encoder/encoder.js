#!/usr/bin/env node

// See IICS26RouterMsgs.sol for the ABI
// This script encodes/decodes packet between JSON and ABI
// Example ETH -> Cosmos TX
// @see https://dashboard.tenderly.co/tx/0x6110883c06d5e31bcaa08e98a940f452555f515cdf15f4ae5427c31ddcc7fdca/logs
//
// struct Packet {
//    uint64 sequence;
//    string sourceClient;
//    string destClient;
//    uint64 timeoutTimestamp;
//    Payload[] payloads;
// }
//
// struct Payload {
//    string sourcePort;
//    string destPort;
//    string version;
//    string encoding;
//    bytes value;
// }

const { ethers } = require('ethers');
const fs = require('fs');

function showUsage() {
    console.log('Usage:');
    console.log('./encoder.js encode <json-file> [--base64]');
    console.log('./encoder.js decode <hex-string>');
    process.exit(1);
}

function encodePacket(jsonFile, outputBase64 = false) {
    // Encode the packet to ABI hex
    const coder = new ethers.AbiCoder();

    try {
        const packet = JSON.parse(fs.readFileSync(jsonFile, 'utf8'));

        const abiHex = coder.encode(
            ['tuple(uint64,string,string,uint64,tuple(string,string,string,string,bytes)[])'],
            [[
                packet.sequence,
                packet.sourceClient,
                packet.destClient,
                packet.timeoutTimestamp,
                packet.payloads.map(p => [p.sourcePort, p.destPort, p.version, p.encoding, p.value])
            ]]
        );

        if (!outputBase64) {
            console.log(abiHex);
            return
        }

        // Remove 0x prefix and convert hex to base64
        const hexData = abiHex.startsWith('0x') ? abiHex.slice(2) : abiHex;
        const base64Data = Buffer.from(hexData, 'hex').toString('base64');
        console.log(base64Data);
    } catch (error) {
        console.error('Error:', error.message);
        process.exit(1);
    }
}

function decodePacket(hexString) {
    let abiHex = hexString;
    if (abiHex.startsWith('0x')) {
        abiHex = abiHex.slice(2);
    }

    const coder = new ethers.AbiCoder();

    try {
        const decoded = coder.decode(
            ['tuple(uint64 sequence, string sourceClient, string destClient, uint64 timeoutTimestamp, tuple(string sourcePort, string destPort, string version, string encoding, bytes value)[] payloads)'],
            '0x' + abiHex
        );

        // Convert to readable JSON
        const packet = {
            sequence: Number(decoded[0][0]),
            sourceClient: decoded[0][1],
            destClient: decoded[0][2],
            timeoutTimestamp: Number(decoded[0][3]),
            payloads: decoded[0][4].map(payload => ({
                sourcePort: payload[0],
                destPort: payload[1],
                version: payload[2],
                encoding: payload[3],
                value: payload[4]
            }))
        };

        console.log(JSON.stringify(packet, null, 2));

    } catch (error) {
        console.error('Error:', error.message);
        process.exit(1);
    }
}

if (process.argv.length < 4) {
    showUsage();
}

const command = process.argv[2];
const arg = process.argv[3];
const options = process.argv.slice(4);

if (command === 'encode') {
    const base64Output = options.includes('--base64');
    encodePacket(arg, base64Output);
} else if (command === 'decode') {
    decodePacket(arg);
} else {
    console.error(`Unknown command: ${command}`);
    showUsage();
}