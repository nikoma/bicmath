// Web Worker: heavier requests run here so the main thread stays responsive.
// The worker loads the same WASM engine module as the page.

import init, { BicMath } from "./pkg-web/bicmath_wasm.js";

const ready = init().then(() => new BicMath());

self.onmessage = async (event) => {
  const engine = await ready;
  const message = event.data;
  try {
    if (typeof message.expression === "string") {
      self.postMessage(JSON.parse(engine.evaluateJson(JSON.stringify(message))));
    } else {
      self.postMessage(JSON.parse(engine.calculateJson(JSON.stringify(message))));
    }
  } catch (error) {
    self.postMessage({ error: String(error) });
  }
};
