# Tutorial: Securely Deploying monero-wallet-rpc Without Spend Keys

When deploying a Monero payment gateway or monitoring service in a production environment, security is the top priority. To prevent funds from being stolen if the server is compromised, **NEVER** store the **Secret Spend Key** on a hot wallet server.

This tutorial will teach you how to deploy `monero-wallet-rpc` in **View-Only** mode. In this mode, the server can detect incoming payments but cannot send funds out.

---

## ⚠️ IMPORTANT: Test on Stagenet First

Before handling real funds (Mainnet), it is strongly recommended to test the entire workflow on the **Stagenet** network.

*   **Stagenet** mimics all the technical features of the Mainnet, but the coins have no value.
*   You can obtain Stagenet test coins from official faucets.
*   **Only switch to Mainnet after you have confirmed that your RPC calls, callback processing, and wallet generation flows are working correctly.**

---

## Step 1: Export the Secret View Key

The core of deploying a View-Only wallet is obtaining the **Standard Address** and the **Secret View Key**.

### Scenario A: Using a Standard Mnemonic Wallet (Software)

If you are using the official Monero GUI Wallet or CLI Wallet:

1.  Open your wallet and log in.
2.  **For GUI Wallet**:
    *   Go to **"Settings"** -> **"Seed & Keys"**.
    *   Enter your wallet password.
    *   Copy the **Primary Address**.
    *   Copy the **Secret View Key** (Note: Make sure it is the *Secret* View Key, not the Public one).
3.  **For CLI Wallet**:
    *   Type `address` to get the primary address.
    *   Type `viewkey` to get the secret view key.

### Scenario B: Using a Hardware Wallet (e.g., Trezor)

Private keys in a hardware wallet never leave the device. To deploy a watch-only wallet, we need to use the official `trezorctl` command-line tool to safely export the view key.

**Prerequisites:**
*   Python environment installed.
*   Trezor library installed: `pip install trezor`
*   Trezor device connected and unlocked.

**Export Command:**

Choose the appropriate parameter based on your target network (Testnet or Mainnet):

```bash
# The path m/44'/128'/0' is the standard derivation path for Monero

trezorctl monero get-watch-key -n "m/44'/128'/0'" --network-type [MAINNET|TESTNET|STAGENET|FAKECHAIN]
```

*   **For Stagenet Testing**: Change the last parameter to `STAGENET`.
*   **For Mainnet Production**: Change the last parameter to `MAINNET`.

The terminal will output your **Address** and **Secret View Key**.

---

## Step 2: Generate the Watch-Only Wallet File

`monero-wallet-rpc` requires a wallet file (`.keys`) to run. We cannot pass the raw keys directly to the RPC; we must first generate this file using the `monero-wallet-cli` tool.

Run the following command in your server terminal:

```bash
# Syntax: monero-wallet-cli --generate-from-view-key <new_wallet_filename>

./monero-wallet-cli --generate-from-view-key watch_only_wallet
```

**Interaction Process:**

1.  **Standard address**: Paste the Primary Address obtained in Step 1.
2.  **Secret view key**: Paste the Secret View Key obtained in Step 1.
3.  **Enter a new password for the wallet**: Set a strong password (required when starting the RPC).
4.  **Restore from specific blockchain height**:
    *   It is recommended to enter the block height from when the wallet was created (enter `0` if unknown, but syncing will take longer).
    *   This determines where the wallet starts scanning for transactions.

Once completed, `watch_only_wallet` and `watch_only_wallet.keys` will be generated in the current directory.

---

## Step 3: Start the View-Only monero-wallet-rpc

Now that we have the secure wallet file, we can start the RPC service.

### Startup Command Example

```bash
./monero-wallet-rpc \
  --daemon-address 127.0.0.1:38081 \
  --wallet-file /path/to/watch_only_wallet \
  --password "YOUR_WALLET_PASSWORD" \
  --rpc-bind-port 18082 \
  --disable-rpc-login \
  --log-file monero-wallet-rpc.log
```

**Parameter Explanation:**

*   `--daemon-address`: Points to your Monero node (monerod). For Stagenet, the port is usually 38081; for Mainnet, it is usually 18081.
*   `--wallet-file`: Points to the watch-only wallet file generated in Step 2.
*   `--password`: The password you set during the wallet generation.
*   `--rpc-bind-port`: The port where the RPC service will listen.
*   `--disable-rpc-login`: Disables RPC username/password login (suitable for trusted internal networks only). For public or higher security setups, use `--rpc-login username:password`.

### Verification

After successful startup, you can test by calling RPC methods like `get_balance` or `query_key`.

If you attempt to call the `transfer` method, the RPC should return an error stating that the wallet is **View-only** and cannot sign transactions. This confirms your deployment is secure.

---

## Summary

By following these steps, you have successfully deployed a Monero wallet service that does not contain spend keys.

*   ✅ **Security**: Even if the server is fully compromised by hackers, they can only view your balance but cannot transfer a single penny.
*   ✅ **Hardware Wallet Support**: Combined with hardware wallets like Trezor, this achieves the highest security level of cold storage where private keys never touch the internet.

**Reminder: Please complete all integration tests on Stagenet first!**
