// Example protocol plugin for the M3 sandboxed JavaScript runtime.
// This file is committed as an example only; it is not executed by the host
// unless a plugin manifest points at a copy of it.
uni.log("info", "example-protocol loaded");
uni.register({
  id: "vnd.example.helloworld",
  kind: "protocol",
  description: "Example protocol plugin"
});
