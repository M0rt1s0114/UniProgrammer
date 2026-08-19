/* global uni */
const adapters = uni.hal.adapters();
uni.log("info", `script-protocol sees ${adapters.length} adapter(s)`);
if (adapters.length > 0) {
  const first = adapters[0];
  const device = first.devices[0];
  uni.hal.open(first.name, device.id);
  const out = uni.hal.call(first.name, device.id, { write: [0x9f], readLen: 3 });
  const hex = out.data.map((b) => b.toString(16).padStart(2, "0")).join(" ");
  uni.log("info", `JEDEC ${hex}`);
  uni.hal.close(first.name, device.id);
  uni.register({ id: "vnd.example.sidecar-protocol", kind: "protocol", description: "Sidecar HAL example" });
}
