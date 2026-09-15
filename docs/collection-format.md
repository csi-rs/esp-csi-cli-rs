# Reading a collection

## What the first rows of a collection are

Every collection begins with rows that look wrong: a large arbitrary number in the leading field, and
a CSI payload length that differs from the rest, before settling into a clean `0, 1, 2, 3, …`
sequence at a consistent length. **None of these are dummy or initialization packets — every one is a
real CSI report.** Two things explain the appearance:

**The leading field is not a packet counter.** It is `sequence_number`, the raw 802.11
sequence-control value of the received frame. That is per-transmitter *and* per-TID, so it does not
start at zero and does not share a counter with your traffic. Your data flood appears to "start at 0"
because QoS data frames use their own TID counter, separate from the management frames that precede
them.

**A different payload length is a different PHY, and often a different device.** A collector is
promiscuous; it reports CSI for every frame its radio decodes:

| `csi_data_len` | `sig_mode` | what it is |
|---|---|---|
| 128 | 0 (non-HT) | the L-LTF-only 64-subcarrier estimate: beacons, auth/assoc, ACKs — **and any third-party device on your channel** |
| 256 / 384 | 1 (HT) | your configured traffic (256 = HT20 L-LTF + HT-LTF, 384 = HT40) |

To keep only your own traffic, either filter **on the device** with
`set-csi-filter --peer-mac=<ap-mac> --min-phy=ht` (which also returns console bandwidth, since
rejected frames are never formatted), or filter on the host: in `array-list` mode the source MAC is
the **last field on each line**, after the payload array, and `sig_mode` / `csi_data_len` identify
the PHY. Filtering by MAC is the reliable one — a busy channel will always give you third-party
frames otherwise.
