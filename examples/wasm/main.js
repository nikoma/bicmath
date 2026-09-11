// Browser example: exact decimals, statistics, and Web Worker execution.
// The WASM module is the same engine used by the native CLI and MCP server.

import init, { BicMath } from "./pkg-web/bicmath_wasm.js";

const output = document.getElementById("output");
const requestBox = document.getElementById("request");

let engine;

function show(value) {
  output.textContent =
    typeof value === "string" ? value : JSON.stringify(value, null, 2);
}

function calculate(request) {
  return JSON.parse(engine.calculateJson(JSON.stringify(request)));
}

async function main() {
  await init();
  engine = new BicMath();
  show(`BicMath ${engine.version} ready; wire schema v${engine.wireSchemaVersion}`);

  document.getElementById("exact").addEventListener("click", () => {
    show(calculate({
      function: "arithmetic.add",
      arguments: { a: "0.1", b: "0.2" },
    }));
  });

  document.getElementById("big").addEventListener("click", () => {
    show(calculate({
      function: "arithmetic.add",
      arguments: { a: { kind: "integer", value: "9007199254740993" }, b: 1 },
    }));
  });

  document.getElementById("median").addEventListener("click", () => {
    show(calculate({
      function: "statistics.median",
      arguments: { values: [1, 2, 10, 100] },
    }));
  });

  document.getElementById("doctor").addEventListener("click", () => {
    show(JSON.parse(engine.doctorJson()));
  });

  document.getElementById("worker").addEventListener("click", () => {
    const worker = new Worker("./worker.js", { type: "module" });
    output.textContent = "running in worker…";
    worker.onmessage = (event) => {
      show(event.data);
      worker.terminate();
    };
    worker.postMessage(JSON.parse(requestBox.value));
  });

  // Evaluate the textarea on Ctrl/Cmd+Enter.
  requestBox.addEventListener("keydown", (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      show(JSON.parse(engine.evaluateJson(requestBox.value)));
    }
  });
}

main().catch((error) => {
  show(`failed to start: ${error}`);
});
