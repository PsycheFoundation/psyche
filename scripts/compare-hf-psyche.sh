#!/usr/bin/env bash

set -euo pipefail

mkdir -p ./txt
rm -rf ./txt/hf
rm -rf ./txt/psyche

mkdir -p ./txt/hf
mkdir -p ./txt/psyche

test_args=(--model emozilla/llama2-20m-init --data-path ./data/fineweb-10bt/ --total-batch 2 --micro-batch 1  --total-steps 2 --cpu --print-tensors)

RUSTFLAGS=-Awarnings cargo run --release -q --bin train -- "${test_args[@]}" | csplit -f "./txt/psyche/part" -  "/^.*:.*$/+0" "{*}" > /dev/null &
cargo_pid=$!

python train-dupe.py "${test_args[@]}" | csplit -f "./txt/hf/part" - "/^.*:.*$/+0" "{*}" > /dev/null &
python_pid=$!

echo "started python & rust runs..."
trap 'kill $python_pid $cargo_pid 2>/dev/null; echo "Caught Ctrl-C, killing processes..."; exit 1' INT
wait $python_pid && echo "done python run"
wait $cargo_pid && echo "done cargo run"

echo "done running scripts!"

echo "checking for diffs..."
echo "" > ./txt/diffs.txt

hasdiff=0

for hf_file in $(ls ./txt/hf/part* | sort -V); do
    psyche_file="./txt/psyche/$(basename $hf_file)"
    if [ -f "$psyche_file" ]; then
        hf_sum=$(sha1sum "$hf_file" | cut -d' ' -f1)
        psyche_sum=$(sha1sum "$psyche_file" | cut -d' ' -f1)

        if [ "$hf_sum" != "$psyche_sum" ]; then
            first_line=$(head -n 1 $hf_file)
            echo "$first_line -- diff $hf_file $psyche_file -y | less" >> ./txt/diffs.txt
            hasdiff=1
        fi
    fi
done

if [ "$hasdiff" -eq "1" ]; then
    echo "diffs available for viewing at ./txt/diffs.txt"
    less ./txt/diffs.txt
else
    echo "no diffs found!! YAY"
fi