#!/usr/bin/env sh
# Offline smoke test: no network, no account, no API keys.
# Builds the release CLI and exercises exact arithmetic, statistics, finance,
# units, linear algebra, scientific functions, and the MCP doctor report.
set -eu
cd "$(dirname "$0")/.."

cargo build --release -p bicmath

BICMATH=target/release/bicmath

echo "==> doctor"
"$BICMATH" doctor --format text

echo "==> exact decimal"
echo '{"function":"arithmetic.add","arguments":{"a":"0.1","b":"0.2"}}' \
  | "$BICMATH" calculate | grep -q '"value": "0.3"'
echo "0.1 + 0.2 = 0.3 (exact)"

echo "==> large integer"
echo '{"function":"arithmetic.add","arguments":{"a":{"kind":"integer","value":"9007199254740993"},"b":1}}' \
  | "$BICMATH" calculate | grep -q '9007199254740994'
echo "9007199254740993 + 1 = 9007199254740994 (exact)"

echo "==> statistics"
echo '{"function":"statistics.median","arguments":{"values":[1,2,10,100]}}' \
  | "$BICMATH" calculate | grep -q '"value": "6"'
echo "median([1,2,10,100]) = 6"

echo "==> finance"
echo '{"expression":"finance.npv(rate = 0.08, cashflows = [-10000, 4000, 4000, 4000], currency = \"USD\")"}' \
  | "$BICMATH" evaluate | grep -q '"npv"'
echo "npv computed"

echo "==> units"
echo '{"function":"units.convert","arguments":{"value":{"kind":"quantity","value":{"kind":"decimal","value":"0"},"dimension":{"temperature":1}},"from":"degC","to":"K"}}' \
  | "$BICMATH" calculate | grep -q '273.15'
echo "0 Celsius = 273.15 Kelvin"

echo "==> linear algebra"
echo '{"function":"linear_algebra.determinant","arguments":{"matrix":{"kind":"matrix","rows":2,"cols":2,"data":[1,2,3,4]}}}' \
  | "$BICMATH" calculate | grep -q '"-2"'
echo "det([[1,2],[3,4]]) = -2"

echo "==> scientific"
echo '{"function":"scientific.integrate","arguments":{"expression":"x^2","variable":"x","lower":0,"upper":1},"context":{"mode":"scientific"}}' \
  | "$BICMATH" calculate | grep -q '"integral"'
echo "integral of x^2 on [0,1] computed"

echo "offline smoke test passed"
