#!/usr/bin/env bash
set -euo pipefail

out="${1:-test/cometbft/fixtures/native_ics23_parser_fixture.json}"

cast_sig='f(((uint8,(bytes,bytes,bool,(uint8,uint8,uint8,uint8,bytes),(uint8,bytes,bytes)[]),(bytes,(bool,(bytes,bytes,bool,(uint8,uint8,uint8,uint8,bytes),(uint8,bytes,bytes)[])),(bool,(bytes,bytes,bool,(uint8,uint8,uint8,uint8,bytes),(uint8,bytes,bytes)[]))))[]))'

empty_exist='(0x,0x,false,(0,0,0,0,0x),[])'
empty_non="(0x,(false,${empty_exist}),(false,${empty_exist}))"
iavl_leaf_prefix='0x000000'
iavl_inner_prefix='0x02000020'
tendermint_leaf_prefix='0x00'
tendermint_inner_prefix='0x01'
iavl_leaf="(1,0,1,1,${iavl_leaf_prefix})"
iavl_path_ops="[(1,${iavl_inner_prefix},0x)]"
tendermint_leaf="(1,0,1,1,${tendermint_leaf_prefix})"
tendermint_path_ops="[(1,${tendermint_inner_prefix},0x)]"

ibc='0x696263'
client='0x636c69656e74732f30372d74656e6465726d696e742d302f636c69656e745374617465'
missing='0x636c69656e74732f30372d74656e6465726d696e742d302f6d697373696e67'
next='0x636c69656e74732f30372d74656e6465726d696e742d302f6e657874'
value='0x76616c7565'

strip_0x() {
  printf '%s' "${1#0x}"
}

proto_varint_hex() {
  local value="$1"
  local out=""
  local byte
  while (( value >= 128 )); do
    byte=$(( (value & 0x7f) | 0x80 ))
    out="${out}$(printf '%02x' "${byte}")"
    value=$(( value >> 7 ))
  done
  printf '%s%02x' "${out}" "${value}"
}

hex_len_varint() {
  local hex
  hex="$(strip_0x "$1")"
  proto_varint_hex "$(( ${#hex} / 2 ))"
}

sha256_hex() {
  local hex
  hex="$(strip_0x "$1")"
  printf '%s' "${hex}" | xxd -r -p | openssl dgst -sha256 -binary | xxd -p -c 256 | sed 's/^/0x/'
}

apply_leaf() {
  local prefix="$1"
  local key="$2"
  local val="$3"
  local val_hash
  val_hash="$(sha256_hex "${val}")"
  sha256_hex "0x$(strip_0x "${prefix}")$(hex_len_varint "${key}")$(strip_0x "${key}")20$(strip_0x "${val_hash}")"
}

apply_inner() {
  local prefix="$1"
  local child="$2"
  local suffix="$3"
  sha256_hex "0x$(strip_0x "${prefix}")$(strip_0x "${child}")$(strip_0x "${suffix}")"
}

store_root="$(apply_inner "${iavl_inner_prefix}" "$(apply_leaf "${iavl_leaf_prefix}" "${client}" "${value}")" "0x")"
app_root="$(apply_inner "${tendermint_inner_prefix}" "$(apply_leaf "${tendermint_leaf_prefix}" "${ibc}" "${store_root}")" "0x")"

membership="[(1,(${client},${value},true,${iavl_leaf},${iavl_path_ops}),${empty_non}),(1,(${ibc},${store_root},true,${tendermint_leaf},${tendermint_path_ops}),${empty_non})]"
non_membership="[(2,${empty_exist},(${missing},(false,${empty_exist}),(true,(${next},${value},true,${iavl_leaf},${iavl_path_ops})))),(1,(${ibc},${store_root},true,${tendermint_leaf},${tendermint_path_ops}),${empty_non})]"

membership_proof="$(cast abi-encode "${cast_sig}" "(${membership})")"
non_membership_proof="$(cast abi-encode "${cast_sig}" "(${non_membership})")"

mkdir -p "$(dirname "${out}")"
cat > "${out}" <<JSON
{
  "membership": {
    "path": [
      "${ibc}",
      "${client}"
    ],
    "value": "${value}",
    "root": "${app_root}",
    "proof": "${membership_proof}"
  },
  "nonMembership": {
    "path": [
      "${ibc}",
      "${missing}"
    ],
    "proof": "${non_membership_proof}"
  }
}
JSON

echo "${out}"
