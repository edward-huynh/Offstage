#!/bin/sh
# Converts the official vinai/PhoWhisper-small (BSD-3-Clause) to ggml and quantizes it to q5_1,
# the same format as the default ggml-small-q5_1.bin. Everything is pinned, and the result must
# match the checksum below, so anyone can check where the file came from.
#
# Needs: git, curl, python3 (3.9 or newer), cmake and a C++ toolchain. It downloads about
# 1 GB of weights and 600 MB of Python packages, all inside <work-dir>. The weights are a pickle
# file, so they are only ever opened with torch.load(weights_only=True).
#
# Usage: ./convert-phowhisper.sh <work-dir>
# Result: <work-dir>/ggml-phowhisper-small-q5_1.bin
set -eu

work="${1:?usage: convert-phowhisper.sh <work-dir>}"
mkdir -p "$work"
work=$(cd "$work" && pwd)

HF_COMMIT=a86b604c346caf7148c37512eafe783a16420adb
WEIGHTS_SHA256=9ec6f07c5bd321fc8f477cf778c43c3fd94b0e33a9f20283fc61cc6d22cbfded
WHISPER_CPP_COMMIT=2eeeba56e9edd762b4b38467bab96c2517163158 # tag v1.8.3, the version whisper-rs uses
OPENAI_WHISPER_COMMIT=86098128c0b4f24f0e2aa2994de830614b474227 # only for whisper/assets/mel_filters.npz
EXPECTED_SHA256=c93fb04f6f586de4eb9b82501f371d4b0bdd033e266264a5ab4f787dea25343e

sha256() {
  if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1"; else sha256sum "$1"; fi | cut -d' ' -f1
}

# Fetches exactly one commit of a repository into a fresh directory.
fetch_commit() { # url commit dir
  if [ ! -d "$3/.git" ]; then
    git init -q "$3"
    git -C "$3" fetch -q --depth 1 "$1" "$2"
    git -C "$3" checkout -q FETCH_HEAD
  fi
  [ "$(git -C "$3" rev-parse HEAD)" = "$2" ] || { echo "unexpected commit in $3" >&2; exit 1; }
}

echo "1/6 weights from Hugging Face (commit $HF_COMMIT)"
model="$work/src-model"
mkdir -p "$model"
for f in added_tokens.json config.json generation_config.json merges.txt normalizer.json \
  preprocessor_config.json special_tokens_map.json tokenizer.json tokenizer_config.json vocab.json \
  pytorch_model.bin; do
  [ -f "$model/$f" ] || curl -sfL --retry 3 -o "$model/$f" \
    "https://huggingface.co/vinai/PhoWhisper-small/resolve/$HF_COMMIT/$f"
done
[ "$(sha256 "$model/pytorch_model.bin")" = "$WEIGHTS_SHA256" ] || { echo "weights checksum mismatch" >&2; exit 1; }

echo "2/6 whisper.cpp and OpenAI Whisper sources"
fetch_commit https://github.com/ggml-org/whisper.cpp "$WHISPER_CPP_COMMIT" "$work/whisper.cpp"
fetch_commit https://github.com/openai/whisper "$OPENAI_WHISPER_COMMIT" "$work/openai-whisper"

echo "3/6 isolated Python environment"
if [ ! -x "$work/venv/bin/python" ]; then
  python3 -m venv "$work/venv"
  "$work/venv/bin/python" -I -m pip install -q --upgrade pip
  "$work/venv/bin/python" -I -m pip install -q torch==2.8.0 transformers==4.57.6 numpy==2.0.2
fi

echo "4/6 opening the weights in safe mode"
"$work/venv/bin/python" -I -c '
import sys, torch
# weights_only=True accepts tensors and plain containers only, so code hidden in a pickle fails here.
state = torch.load(sys.argv[1], map_location="cpu", weights_only=True)
print("  safe load ok:", len(state), "tensors")
' "$model/pytorch_model.bin"

echo "5/6 converting to ggml (f16)"
mkdir -p "$work/ggml-f16"
"$work/venv/bin/python" -I "$work/whisper.cpp/models/convert-h5-to-ggml.py" \
  "$model" "$work/openai-whisper" "$work/ggml-f16" >/dev/null

echo "6/6 quantizing to q5_1"
if [ ! -x "$work/build/bin/whisper-quantize" ]; then
  cmake -S "$work/whisper.cpp" -B "$work/build" -DCMAKE_BUILD_TYPE=Release \
    -DWHISPER_BUILD_EXAMPLES=ON -DWHISPER_BUILD_TESTS=OFF -DGGML_METAL=OFF -DWHISPER_SDL2=OFF >/dev/null
  cmake --build "$work/build" --target whisper-quantize -j 8 >/dev/null
fi
out="$work/ggml-phowhisper-small-q5_1.bin"
"$work/build/bin/whisper-quantize" "$work/ggml-f16/ggml-model.bin" "$out" q5_1 >/dev/null 2>&1

actual=$(sha256 "$out")
echo "result:   $out"
echo "sha256:   $actual"
if [ "$actual" = "$EXPECTED_SHA256" ]; then
  echo "matches the expected checksum"
else
  echo "DOES NOT match the expected $EXPECTED_SHA256 (different tool or package versions?)" >&2
  exit 1
fi
