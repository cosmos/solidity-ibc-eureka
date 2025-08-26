openssl ecparam -name secp256k1 -genkey -noout -out ec-secp256k1-priv-key.pem &&\
# 2) Convert your PEM EC key to unencrypted PKCS#8 DER
openssl pkcs8 \
  -topk8 \
  -nocrypt \
  -in ec-secp256k1-priv-key.pem \
  -outform DER \
  -out key-pk8.der &&\
# 3) Inspect its ASN.1 structure to locate the privateKey OCTET STRING
openssl asn1parse \
  -in key-pk8.der \
  -inform DER \
  -i &&\
# 4) Extract that SEC1 blob at offset 26
openssl asn1parse \
  -in key-pk8.der \
  -inform DER \
  -strparse 26 \
  -out sec1.der \
  -noout &&\
# 5) Inspect the SEC1 DER to find the inner 32-byte OCTET STRING
openssl asn1parse \
  -in sec1.der \
  -inform DER \
  -i &&\
# 6) Dump that 32-byte scalar into key.bin
openssl asn1parse \
  -in sec1.der \
  -inform DER \
  -strparse 5 \
  -out key.bin \
  -noout &&\
echo "Key created under 'key.bin'" &&\
rm ec-secp256k1-priv-key.pem key-pk8.der sec1.der
