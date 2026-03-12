---
title: Exporting Wallet to MetaMask
sidebar_position: 4
---

:::tip

Calibnet and mainnet wallets are mostly interchangeable; under the hood they have the same private key. You can export a calibnet wallet and import it into MetaMask, then switch to mainnet in MetaMask and it will work just fine. The only difference is that calibnet wallets start with `t` prefix and mainnet wallets start with `f`. Some tools might reject `t...` addresses when mainnet is selected, some will happily accept either.

:::

# Exporting a Forest Wallet to MetaMask

This guide walks you through exporting a Forest wallet (`f4` address) to MetaMask, allowing you to manage your Filecoin account through the MetaMask browser extension.

## Prerequisites

- A Forest wallet with an `f4` address (`EVM`-compatible address)
- MetaMask browser extension installed
- Command-line tools: `forest-wallet`, `xxd`, `jq`, and `base64`

## Step 1: Export the Private Key

:::info

You can list your wallets and their balances using `forest-wallet list`. Make sure to identify the correct `f4` address you want to export. Refer to the [wallets guide](./interacting_with_wallets) for more details on managing wallets in Forest.

:::

To export your wallet's private key in a format compatible with MetaMask, use the following command:

```shell
forest-wallet export f4... | xxd -r -p | jq -r '.PrivateKey' | base64 -d | xxd -p -c 32
```

Replace `f4...` with your actual `f4` address.

This command will output a hexadecimal private key that can be imported into MetaMask.

:::danger

**Keep your private key secure!** Never share your private key with anyone. Anyone with access to your private key has full control over your funds. Make sure to run this command in a secure environment and clear your terminal history afterward.

:::

## Step 2: Add Filecoin Network to MetaMask

Before importing your account, you need to add the Filecoin network to MetaMask:

### For Mainnet

Visit [Chainlist - Filecoin Mainnet](https://chainlist.org/chain/314) and click "Connect Wallet" to automatically add the network configuration to MetaMask.

### For Calibnet

Visit [Chainlist - Filecoin Calibration Network](https://chainlist.org/chain/314159) and click "Connect Wallet" to automatically add the network configuration to MetaMask.

:::tip

You can also manually add the network in MetaMask by going to "Networks → Add a Custom Network" and entering the network details from Chainlist.

:::

## Step 3: Import the Account into MetaMask

1. Open MetaMask in your browser
2. Click on the account icon in the top-right corner
3. Select **"Add wallet"**
4. Choose **"Import an account"**
5. Paste the hexadecimal private key from Step 1
6. Click **"Import"**

Your Forest wallet should now be accessible in MetaMask!

## Step 4: Verify the Import

1. Switch to the Filecoin network in MetaMask (mainnet or calibnet, depending on which you added)
2. Verify that the account address matches your original `f4` address. MetaMask will display the address in Ethereum format (`0x...`), but it corresponds to the same account on the Filecoin network - by, e.g., going to a block explorer like [Blockscout](https://filecoin.blockscout.com/) and searching for your `f4` address, then comparing the `0x` address shown in Blockscout with the one in MetaMask. You can also use the [Beryx address converter tool](https://beryx.io/address_converter).
3. Check that your balance is displayed correctly

## Security Best Practices

- **Never share your private key** with anyone or any service
- **Use a hardware wallet** for large amounts of FIL
- **Test with small amounts** first on calibnet before using mainnet
- **Keep backups** of your wallet in a secure location

## Troubleshooting

### My balance doesn't appear in MetaMask

Ensure you've switched to the correct Filecoin network (mainnet or calibnet) in MetaMask. The network must match where your funds are located.

### MetaMask shows a different address format

MetaMask displays addresses in Ethereum format (`0x...`). This is normal - it's the same account, just displayed in a different address format. The `f4` address and the `0x` address represent the same account on the Filecoin network.

## Related Resources

- [Interacting with Wallets](./interacting_with_wallets)
- [Filecoin Address Types](https://docs.filecoin.io/smart-contracts/filecoin-evm-runtime/address-types)
