## Differences between serializers

The serializer created here uses `multihash` and `libipld-json` uses plain `base64`. That means one has an extra `m` in front of all the encoded byte values, using our serializer. For example:

|`forest_ipld`|`libipld-json`|
| ----------- | ------------ |
|```{ "/": { "bytes": "mVGhlIHF1aQ" } }```|```{ "/": { "bytes": "VGhlIHF1aQ" } }```|

As `Lotus` is using `multihash-base64` and our serializer is using it as well.