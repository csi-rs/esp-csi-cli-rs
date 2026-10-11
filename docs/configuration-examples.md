# Configuration examples

Worked setups, each a sequence of console commands. The terms used here
(mode, session role, sensing role, reporting — IEEE 802.11bf terms) are defined in the
[node model](https://github.com/csi-rs/esp-csi-rs/blob/main/docs/network-model.md).

## CLI Configuration Examples

1.  **Configure an ESP as a WiFi Sniffer on channel 6 and collect indefinitely in array-list format:**
    ```
    set-wifi --mode=sniffer --set-channel=6
    set-log-mode --mode=array-list
    show-config
    start
    ```

2.  **Configure an ESP as a Station connected to an existing network and collect for 5 minutes:**
    ```
    set-wifi --mode=station --sta-ssid="My Router" --sta-password="router password"
    set-measurement --periodicity-hz=20
    show-config
    start --duration=300
    ```

3.  **Emitter + sniffer (any chip — the controlled pairing):**
    ```
    # Sniffer board (an observer that reports, on the emitter's channel, no self-generated traffic)
    set-wifi --mode=sniffer --set-channel=6
    set-measurement --periodicity-hz=0
    set-log-mode --mode=array-list
    start

    # Emitter board (a transmitter that reports none; use ht40-emitter for 40 MHz bonded)
    set-wifi --mode=ht20-emitter --set-channel=6 --inject-period-us=20000
    start
    ```
    The emitter sources the traffic and captures nothing; the sniffer measures
    it. An emitter never associates and its frames carry no payload meaning, so
    one emitter sounds any number of sniffers at once; unicasting with
    `--peer-mac` to one sniffer tends to raise that sniffer's CSI rate.

4.  **ESP-NOW initiator + responder (any chip — connectionless, no AP needed):**
    ```
    # Initiator board (sources the traffic)
    set-wifi --mode=esp-now-initiator --set-channel=6
    set-rate --rate=mcs0-lgi
    set-log-mode --mode=array-list
    start

    # Responder board (replies; both ends report by default)
    set-wifi --mode=esp-now-responder --set-channel=6
    set-rate --rate=mcs0-lgi
    set-log-mode --mode=array-list
    start
    ```
    Both boards must be on the same `--set-channel`. Pairing is automatic; with more
    than two boards on the channel, set `--peer-mac` on both to pin the pair. Either
    end can be set to report nothing with `set-wifi --reporting=none`; it still
    captures, but reports nothing. A responder whose initiator reports none
    promotes itself to `report` so the pair still produces data.

5.  **Keep a node's traffic on air without reporting its CSI, then check stats:**
    ```
    set-wifi --mode=wifi-ap --set-channel=6 --reporting=none
    set-measurement --periodicity-hz=4000
    start
    # ... after pressing 'q' to stop:
    show-stats
    ```
    `--reporting=none` keeps the node part of the network and capturing, but
    reporting nothing. To measure capture cost without delivery cost on a
    reporting node instead, keep its reporting and switch off the runtime
    delivery gate with `set-csi-output --enabled=false`.

6.  **Emit ESP32-CSI-Tool-compatible CSV for a host pipeline:**
    ```
    set-wifi --mode=sniffer --set-channel=6
    set-log-mode --mode=esp-csi-tool
    start --duration=60
    ```

7.  **SoftAP lab pair (board A = access point, board B = station on same SSID):**
    ```
    # Board A (access point: the initiator, reports)
    set-wifi --mode=wifi-ap --set-channel=6 --ap-ssid=esp-csi-ap
    set-protocol --protocol=n
    set-measurement --periodicity-hz=4000
    start

    # Board B (station — match AP SSID/channel)
    set-wifi --mode=station --sta-ssid=esp-csi-ap --set-channel=6
    set-protocol --protocol=n
    set-measurement --periodicity-hz=4000
    start
    ```

8.  **5 GHz associated AP/STA pair (ESP32-C5 — serialized high-rate CSI):**
    ```
    # Board A (access point — 5 GHz ch149 on C5)
    set-wifi --mode=wifi-ap --set-channel=149 --ap-ssid=esp-csi-ap
    set-protocol --protocol=n
    set-measurement --periodicity-hz=4000
    start

    # Board B (station RX — record serialized CSI)
    set-wifi --mode=station --sta-ssid=esp-csi-ap --set-channel=149
    set-protocol --protocol=n
    set-log-mode --mode=serialized
    set-io-tasks --tx=off
    start
    ```
    On ESP32-C5 the station channel doubles as the dual-band hint (2.4 GHz:
    `--set-channel=6`, 5 GHz: `--set-channel=149`). Disable station TX
    (`set-io-tasks --tx=off`) so the AP's downlink flood is not competing with a
    second ICMP generator.

    The pair scales to multiple stations: the AP's DHCP pool holds 4 leases by
    default (`set-wifi --ap-leases=<1-8>` to change) and the ICMP flood
    round-robins across all associated stations, so each one captures CSI. Note
    the offered rate is shared — with N stations each sees roughly
    `periodicity-hz / N` packets per second.

    For **temporally-synchronized** multi-receiver captures, add
    `set-wifi --ap-burst=on` on the AP: every flood tick then fires one unicast
    frame back-to-back to each associated station, so all stations sample the
    channel within tens of microseconds of one another and each sees the full
    `periodicity-hz` rate. (A single broadcast frame cannot be used instead — on
    an ESP32 softAP broadcast is DTIM-buffered and dropped under load.) Total
    offered airtime becomes `periodicity-hz × N`, so lower
    `set-measurement --periodicity-hz` if the channel saturates.

9.  **ESP-NOW simplex pair (highest CSI rate of any pairing):**
    ```
    # Peer board (responder, a receiver that reports) — sparse discovery beacon, then RX-only
    set-wifi --mode=esp-now-simplex-peer --set-channel=6
    set-log-mode --mode=serialized
    start

    # Source board (initiator, a transmitter that reports none) — learns the peer's MAC, then unicasts a continuous flood
    set-wifi --mode=esp-now-simplex-source --set-channel=6
    start
    ```
    Asymmetric on purpose: the peer stops transmitting once it has heard the
    source, so all the airtime belongs to a single transmitter. Start the peer
    first so the beacon is already on air when the source comes up. `set-rate` does
    not apply here — the simplex profile fixes its own PHY. To pin the pair
    instead of discovering it, give each board the other's address with
    `--peer-mac`.

    The source is the session's initiator (it sources the traffic) and the peer
    is the responder.
