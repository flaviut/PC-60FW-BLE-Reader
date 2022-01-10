# PC-60FW BLE Pulse Oximeter Software

Connect to your PC-60FW BLE Pulse Oximeter over BLE from your computer.

This software automatically tries to reconnect, since I've found the device's
connection to be fairly unreliable.

There is some sort of bug where the device connects sucessfully but does not
print any readings. I'm not sure what's going on there, and I haven't had a
chance to figure it out. Rebooting works fine.

Run it using `cargo run`

To get debugging messages, set `RUST_LOG=ble_spo2=debug` or
`RUST_LOG=ble_spo2=trace` before running.
