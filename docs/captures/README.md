# Reference captures (own hardware, read-only unless noted)

- `xr18-fw120-baseline.pcap` (2026-10-18 ~03:05 AWST): XR18 "Home Studio"
  @ 192.168.40.123, FW 1.20. 974 frames: /xremote, /xinfo, /info,
  ch01-04/15-16 fader+on+name reads, /lr reads, headamp reads, then
  45 s meter stream (/meters/1, 8 s renewals). LINKTYPE_RAW (101)
  with synthesized IPv4/UDP headers (capture host 192.168.1.13).
  Opens in Wireshark (use the OSC dissector). Write ops in this
  capture: NONE (read-only; same-value write ACKs were tested
  separately, not recorded here).
