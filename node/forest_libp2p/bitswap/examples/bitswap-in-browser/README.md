# Bitswap WASM example

1. Start the server, the program will print its address and some random block
   CIDs that are available in its in-memory blockstore

```console
cargo run
```

2. Start the browser

```console
# Install pnpm(https://pnpm.io/)
# npm i -g pnpm or yarn global add pnpm
# Install wasm-pack
# cargo install --locked wasm-pack
➜  bitswap-in-browser git:(hm/bitswap-example-wasm)
cd wasm
pnpm i
pnpm run build
pnpm run start
```

3. Paste the server address and block cid into the web page
4. Click on `Get block` button
