# Security Analysis: 64-bit Encrypted Payment IDs

## 1. The Vulnerability: Cleartext Legacy PIDs

In the original design of `anon-ticket`, we utilized "Legacy Payment IDs”—arbitrary 32-byte (256-bit) strings attached to Monero transactions. While these offered immense entropy, they suffered from a critical flaw: **Lack of Privacy**.

Legacy Payment IDs are stored in the transaction `extra` field in **cleartext**. This visibility enables a specific class of attack: **Front-Running**.

**Attack Scenario:**
1.  A legitimate user generates a random PID and broadcasts a transaction to buy a ticket.
2.  An attacker monitors the Monero mempool or new blocks.
3.  The attacker sees the pending transaction and extracts the plaintext PID.
4.  The attacker immediately sends a `/redeem` request to our API using that PID.
5.  If the attacker's request reaches our server before the legitimate user's request (or if the user waits for confirmations while the attacker polls aggressively), the attacker steals the service token.

To eliminate this vector, we must switch to **Integrated Addresses**.

## 2. The Solution: Encrypted 64-bit PIDs

Monero Integrated Addresses embed a short, 64-bit (8-byte) Payment ID directly into the destination address. Crucially, this ID is **encrypted** using the transaction's shared secret (ECDH).

*   **Sender**: Encodes the ID into the address.
*   **Blockchain**: Stores only the encrypted ID. To an observer, it looks like random noise.
*   **Receiver (Us)**: Decrypts the ID using the private view key.

This prevents any third party (including the attacker) from seeing the PID on-chain. They cannot front-run the redemption because they don't know *which* ID to redeem.

## 3. Threat Analysis: Is 64-bit Entropy Enough?

Switching to Integrated Addresses forces us to downgrade from 256-bit IDs to 64-bit IDs. Does this introduce a risk of **Brute-Force Guessing**?

**The Threat Model:**
*   **Goal**: An attacker blindly guesses a PID that corresponds to a *currently unpaid* or *unclaimed* order.
*   **Constraint**: Our service runs behind Tor, so we cannot rely on IP-based rate limiting (WAF). We must rely on the sheer size of the search space.

### Mathematical Proof

*   **Total Search Space**: $2^{64} \approx 1.84 \times 10^{19}$ (18.4 Quintillion).
*   **Attack Surface**: Let's assume a massive success scenario where **1,000,000 (1 Million)** valid, unclaimed PIDs exist in our database simultaneously.
*   **Probability of Success (Single Guess)**:
    $$ P = \frac{10^6}{1.84 \times 10^{19}} \approx 5.4 \times 10^{-14} $$

**Time to Crack:**
Even if an attacker commands a botnet capable of sending **10,000 requests per second** to our Tor hidden service (a generous assumption given Tor's latency):

$$ \text{Time to find 1 key} \approx \frac{1}{10,000 \times 5.4 \times 10^{-14}} \approx 1.85 \times 10^9 \text{ seconds} $$

$$ 1.85 \times 10^9 \text{ seconds} \approx \textbf{58 Years} $$

**Conclusion:**
Even without IP rate limiting, the 64-bit space is cryptographically vast enough to make online brute-force attacks economically and practically impossible. The cost of electricity and bandwidth to sustain such an attack for decades far outweighs the value of a service token.

### Birthday Paradox (Internal Collisions)

What about users accidentally generating the same PID?

*   With $2^{64}$ space, the probability of collision reaches 50% only after generating $\approx 4 \times 10^9$ (4 Billion) IDs.
*   Since we only care about collisions among **active (unclaimed)** orders, as long as our active queue stays under ~100 million, accidental collisions are statistically negligible.
*   Even if a collision occurs (two users pick the same ID), the **Transaction ID (TXID)** on the blockchain is unique. Our system tracks payments by `(PID, TXID)`. The only failure mode is if user A pays for PID `X` and user B pays for PID `X`, and user A redeems *user B's* payment. Given the odds, this is an acceptable risk for a "Single-Node" system.

## 4. Workflow: Stateless & Private

To maximize privacy and minimize DoS vectors, we maintain a **Stateless** issuance flow:

1.  **Client-Side Generation**: The user's client generates a random 8-byte PID.
2.  **Local Encoding**: The client combines our public **Standard Address** with this PID to construct an **Integrated Address** locally.
    *   *Benefit*: The server does not know an order exists until payment is made.
3.  **Payment**: The user pays to the Integrated Address.
4.  **Redemption**: The user submits the 8-byte PID (as hex) to our API.
    *   *Validation*: The server checks if this PID exists in the database (populated by the Monitor, which decrypts incoming transfers).

This architecture ensures that **we do not store any state** for unpaid orders, completely eliminating "Order Creation" DoS attacks.
