// Node smoke test for the BicMath WASM build.
//
// This exercises the same wasm module the browser example loads, including a
// worker_threads execution path that mirrors the browser Web Worker example.
//
// Run: node examples/wasm/smoke.cjs

const path = require("node:path");
const { Worker } = require("node:worker_threads");
const assert = require("node:assert");

const pkg = require("./pkg-node/bicmath_wasm.js");

function canonical(value) {
  return JSON.stringify(value);
}

function main() {
  const engine = new pkg.BicMath();
  assert.strictEqual(engine.version.length > 0, true);
  assert.strictEqual(engine.wireSchemaVersion, 1);

  // Exact decimal: 0.1 + 0.2 is exactly 0.3, never 0.30000000000000004.
  const sum = JSON.parse(engine.calculateJson(JSON.stringify({
    function: "arithmetic.add",
    arguments: { a: "0.1", b: "0.2" },
  })));
  assert.strictEqual(canonical(sum.result), canonical({ kind: "decimal", value: "0.3" }));

  // Integer beyond 2^53 survives the JS round trip as strings.
  const big = JSON.parse(engine.calculateJson(JSON.stringify({
    function: "arithmetic.add",
    arguments: { a: { kind: "integer", value: "9007199254740993" }, b: 1 },
  })));
  assert.strictEqual(canonical(big.result), canonical({ kind: "integer", value: "9007199254740994" }));

  // Rational normalization.
  const rational = JSON.parse(engine.calculateJson(JSON.stringify({
    function: "arithmetic.add",
    arguments: {
      a: { kind: "rational", numerator: "1", denominator: "3" },
      b: { kind: "rational", numerator: "1", denominator: "6" },
    },
  })));
  assert.strictEqual(canonical(rational.result), canonical({ kind: "rational", numerator: "1", denominator: "2" }));

  // Statistics function.
  const median = JSON.parse(engine.calculateJson(JSON.stringify({
    function: "statistics.median",
    arguments: { values: [1, 2, 10, 100] },
  })));
  assert.strictEqual(canonical(median.result), canonical({ kind: "integer", value: "6" }));

  // Every module reports and executes in WASM.
  const doctor = JSON.parse(engine.doctorJson());
  assert.strictEqual(doctor.examples_ok, true, JSON.stringify(doctor.example_failures));
  assert.ok(doctor.modules.length >= 6, "expected all modules");
  const perModule = [
    ["arithmetic.add", { a: 1, b: 2 }, undefined],
    ["scientific.sin", { x: 0 }, { mode: "scientific" }],
    ["statistics.median", { values: [1, 3] }, undefined],
    ["finance.break_even", {
      fixed_costs: { kind: "money", amount: { kind: "decimal", value: "1000.00" }, currency: "USD" },
      unit_price: { kind: "money", amount: { kind: "decimal", value: "10.00" }, currency: "USD" },
      unit_variable_cost: { kind: "money", amount: { kind: "decimal", value: "6.00" }, currency: "USD" },
    }, undefined],
    ["units.convert", { value: 1, from: "mile", to: "meter" }, undefined],
    ["linear_algebra.determinant", { matrix: { kind: "matrix", rows: 2, cols: 2, data: [1, 2, 3, 4] } }, undefined],
    ["interval.enclose", { value: "0.1" }, undefined],
    ["verify.equality", { left: "0.1 + 0.2", right: "0.3" }, undefined],
    ["optimize.linear_program", {
      objective: [1, 1],
      constraints: [{ coefficients: [1, 1], relation: "le", rhs: 2 }],
    }, undefined],
    ["algebra.fibonacci", { n: 10 }, undefined],
    ["symbolic.derivative", { expression: "x^2", variable: "x" }, undefined],
    ["geometry.distance_2d", { p: [0, 0], q: [3, 4] }, undefined],
    ["business.eoq", { annual_demand: 10000, order_cost: 50, holding_cost_per_unit: 2 }, undefined],
    ["format.to_markdown", { value: [[1, 2], [3, 4]] }, undefined],
    ["plan.recommend", { task: "compare_proportions" }, undefined],
  ];
  for (const [fn, args, context] of perModule) {
    const request = { function: fn, arguments: args };
    if (context) request.context = context;
    const output = JSON.parse(engine.calculateJson(JSON.stringify(request)));
    assert.ok(output.result, `${fn} returned no result`);
  }

  // Heuristic interval labelling is explicit.
  const interval = JSON.parse(engine.calculateJson(JSON.stringify({
    function: "interval.enclose",
    arguments: { value: "0.1" },
  })));
  assert.strictEqual(interval.result.assurance, "heuristic_padding_not_certified");

  // Exact helper.
  const exact = JSON.parse(pkg.exactAdd("0.1", "0.2"));
  assert.strictEqual(canonical(exact), canonical({ kind: "decimal", value: "0.3" }));

  console.log("wasm smoke test passed: exact decimals, big integers, rationals, statistics, and all modules");
}

function workerTest() {
  return new Promise((resolve, reject) => {
    const worker = new Worker(path.join(__dirname, "worker.cjs"));
    worker.once("message", (message) => {
      const finish = (error) => {
        worker.terminate().finally(() => (error ? reject(error) : resolve()));
      };
      if (message.error) {
        finish(new Error(message.error));
      } else {
        try {
          assert.strictEqual(
            canonical(message.result),
            canonical({ kind: "decimal", value: "0.3" }),
          );
          finish();
        } catch (error) {
          finish(error);
        }
      }
    });
    worker.once("error", reject);
    worker.postMessage({
      request: {
        function: "arithmetic.add",
        arguments: { a: "0.1", b: "0.2" },
      },
    });
  });
}

main();
workerTest()
  .then(() => {
    console.log("worker execution passed: heavy requests run off the main thread");
  })
  .catch((error) => {
    console.error("worker execution failed:", error);
    process.exitCode = 1;
  });
