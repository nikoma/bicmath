// Node worker_threads mirror of the browser Web Worker example.
//
// The worker loads the same wasm module and answers calculate requests, so the
// main thread stays responsive.

const pkg = require("./pkg-node/bicmath_wasm.js");

const engine = new pkg.BicMath();

require("node:worker_threads").parentPort.on("message", (message) => {
  try {
    const output = JSON.parse(
      engine.calculateJson(JSON.stringify(message.request)),
    );
    require("node:worker_threads").parentPort.postMessage({ result: output.result });
  } catch (error) {
    require("node:worker_threads").parentPort.postMessage({ error: String(error) });
  }
});
