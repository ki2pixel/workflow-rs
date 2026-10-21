#!/usr/bin/env bash
# Compile le C API TensorFlow Lite utilise par workflow-tpu.
#
# Le wrapper Python de coral_env n'exporte pas l'API interpreteur. Ce script
# produit un libtensorflowlite_c.so autonome, aligne sur tflite_runtime 2.16.1.
#
# Variables :
#   TF_VERSION     version de TensorFlow (defaut: 2.16.1)
#   CMAKE_VERSION  version de CMake 3.x (defaut: 3.28.6)
#   WORK_DIR       repertoire de travail (defaut: /tmp/opencode/tflite)
#   OUT            chemin de sortie (defaut: <repo>/vendor/libtensorflowlite_c.so)
set -euo pipefail

TF_VERSION="${TF_VERSION:-2.16.1}"
CMAKE_VERSION="${CMAKE_VERSION:-3.28.6}"
WORK_DIR="${WORK_DIR:-/tmp/opencode/tflite}"
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${OUT:-${REPO_DIR}/vendor/libtensorflowlite_c.so}"

mkdir -p "${WORK_DIR}" "$(dirname "${OUT}")"
cd "${WORK_DIR}"

CMAKE_BIN="$(command -v cmake || true)"
if [[ -z "${CMAKE_BIN}" ]] || ! "${CMAKE_BIN}" --version | head -1 | grep -q "version 3\."; then
  CMAKE_DIR="${WORK_DIR}/cmake-${CMAKE_VERSION}-linux-x86_64"
  if [[ ! -x "${CMAKE_DIR}/bin/cmake" ]]; then
    echo "Telechargement de CMake ${CMAKE_VERSION}"
    curl -fsSL -o cmake.tar.gz \
      "https://github.com/Kitware/CMake/releases/download/v${CMAKE_VERSION}/cmake-${CMAKE_VERSION}-linux-x86_64.tar.gz"
    tar xzf cmake.tar.gz
  fi
  CMAKE_BIN="${CMAKE_DIR}/bin/cmake"
fi

if [[ ! -f "tf.tar.gz" ]]; then
  echo "Telechargement de TensorFlow ${TF_VERSION}"
  curl -fsSL -o tf.tar.gz \
    "https://github.com/tensorflow/tensorflow/archive/refs/tags/v${TF_VERSION}.tar.gz"
fi

if [[ ! -d "tensorflow-${TF_VERSION}" ]]; then
  tar xzf tf.tar.gz
fi

BUILD_DIR="build-${TF_VERSION}"
"${CMAKE_BIN}" -S "tensorflow-${TF_VERSION}/tensorflow/lite/c" -B "${BUILD_DIR}" \
  -DCMAKE_BUILD_TYPE=Release \
  -DTFLITE_C_BUILD_SHARED_LIBS=ON

"${CMAKE_BIN}" --build "${BUILD_DIR}" --target tensorflowlite_c -j "$(nproc)"

cp "${BUILD_DIR}/libtensorflowlite_c.so" "${OUT}"
echo "ecrit: ${OUT}"
sha256sum "${OUT}"
nm -D --defined-only "${OUT}" | grep -E "TfLiteInterpreterCreate|TfLiteModelCreateFromFile" | head
