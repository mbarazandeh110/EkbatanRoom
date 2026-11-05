#!/usr/bin/env bash
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

SCENARIO_FILE="${1:-${ROOT_DIR}/scenarios/gate-01-baseline.yaml}"
CONFIG_FILE="${ROOT_DIR}/config.yaml"
BIN_LK="${ROOT_DIR}/bin/lk"
RESULTS_DIR="${ROOT_DIR}/test-results"
EVIDENCE_FILE="${RESULTS_DIR}/loadtest-report.json"

mkdir -p "${RESULTS_DIR}"

if [[ ! -f "${BIN_LK}" ]]; then
    echo "==> [SETUP] lk binary not found in bin/, downloading..."
    mkdir -p "${ROOT_DIR}/bin"
    LK_INSTALL_PATH="${ROOT_DIR}/bin" bash <(curl -sSL https://get.livekit.io/cli)
fi

# بررسی تنظیمات ulimit
CURRENT_ULIMIT=$(ulimit -n)
if [ "${CURRENT_ULIMIT}" -lt 65535 ]; then
    echo "==> [WARN] Current ulimit -n is ${CURRENT_ULIMIT}. For large-scale tests, run: ulimit -n 65535"
fi

# خواندن پارامترها از کانفیگ
LIVEKIT_URL=$(grep "^livekit_url:" "${CONFIG_FILE}" 2>/dev/null | awk '{print $2}' | tr -d '"' || echo "ws://localhost:7880")
API_KEY=$(grep "^api_key:" "${CONFIG_FILE}" 2>/dev/null | awk '{print $2}' | tr -d '"' || echo "devkey")
API_SECRET=$(grep "^api_secret:" "${CONFIG_FILE}" 2>/dev/null | awk '{print $2}' | tr -d '"' || echo "secret")
SFU_METRICS_URL=$(grep "^sfu_metrics_url:" "${CONFIG_FILE}" 2>/dev/null | awk '{print $2}' | tr -d '"' || echo "http://localhost:9090/metrics")
SIG_METRICS_URL=$(grep "^signaling_metrics_url:" "${CONFIG_FILE}" 2>/dev/null | awk '{print $2}' | tr -d '"' || echo "http://localhost:9091/metrics")

# خواندن پارامترهای سناریو
SCENARIO_NAME=$(grep "^name:" "${SCENARIO_FILE}" | awk '{print $2}' | tr -d '"')
PUBLISHERS=$(grep "^publishers:" "${SCENARIO_FILE}" | awk '{print $2}')
SUBSCRIBERS=$(grep "^subscribers:" "${SCENARIO_FILE}" | awk '{print $2}')
DURATION=$(grep "^duration:" "${SCENARIO_FILE}" | awk '{print $2}' | tr -d '"')
RESOLUTION=$(grep "^video_resolution:" "${SCENARIO_FILE}" | awk '{print $2}' | tr -d '"' || echo "high")
CODEC=$(grep "^video_codec:" "${SCENARIO_FILE}" | awk '{print $2}' | tr -d '"' || echo "vp8")
RATE=$(grep "^num_per_second:" "${SCENARIO_FILE}" | awk '{print $2}' || echo "5")
ROOM_PREFIX=$(grep "^room_prefix:" "${SCENARIO_FILE}" | awk '{print $2}' | tr -d '"' || echo "loadtest-room")
ROOM_NAME="${ROOM_PREFIX}-$(date +%s)"

echo "==================================================================="
echo "  EkbatanRoom E2E Load Test: ${SCENARIO_NAME}"
echo "==================================================================="
echo " Target URL   : ${LIVEKIT_URL}"
echo " Room         : ${ROOM_NAME}"
echo " Publishers   : ${PUBLISHERS}"
echo " Subscribers  : ${SUBSCRIBERS}"
echo " Duration     : ${DURATION}"
echo " Resolution   : ${RESOLUTION} (${CODEC})"
echo " Rate         : ${RATE} users/sec"
echo "==================================================================="

START_TIME=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

LK_CMD=(
  "${BIN_LK}" load-test
  --url "${LIVEKIT_URL}"
  --api-key "${API_KEY}"
  --api-secret "${API_SECRET}"
  --room "${ROOM_NAME}"
  --publishers "${PUBLISHERS}"
  --subscribers "${SUBSCRIBERS}"
  --duration "${DURATION}"
  --video-resolution "${RESOLUTION}"
  --video-codec "${CODEC}"
  --num-per-second "${RATE}"
)

# بررسی فلگ سیمولکست
SIMULCAST=$(grep "^simulcast:" "${SCENARIO_FILE}" | awk '{print $2}')
if [ "${SIMULCAST}" = "false" ]; then
    LK_CMD+=(--no-simulcast)
fi

echo "==> Executing: ${LK_CMD[*]}"
RUN_LOG_FILE="${RESULTS_DIR}/${SCENARIO_NAME}-run.log"

TEST_STATUS="PASSED"
set +e
"${LK_CMD[@]}" 2>&1 | tee "${RUN_LOG_FILE}"
EXIT_CODE=$?
set -e

END_TIME=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

if [ ${EXIT_CODE} -ne 0 ]; then
    TEST_STATUS="FAILED"
    echo "==> [ERROR] Scenario failed with exit code ${EXIT_CODE}"
else
    echo "==> [SUCCESS] Scenario completed successfully."
fi

# استخراج داده‌های آماری و ثبت شواهد JSON
cat <<EOF > "${EVIDENCE_FILE}"
{
  "scenario_name": "${SCENARIO_NAME}",
  "start_time": "${START_TIME}",
  "end_time": "${END_TIME}",
  "parameters": {
    "url": "${LIVEKIT_URL}",
    "room": "${ROOM_NAME}",
    "publishers": ${PUBLISHERS},
    "subscribers": ${SUBSCRIBERS},
    "duration": "${DURATION}",
    "resolution": "${RESOLUTION}",
    "codec": "${CODEC}"
  },
  "exit_code": ${EXIT_CODE},
  "gate_verdict": "${TEST_STATUS}",
  "log_file": "${RUN_LOG_FILE}"
}
EOF

echo "==> Evidence saved to: ${EVIDENCE_FILE}"
exit ${EXIT_CODE}
