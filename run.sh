#!/bin/bash

rm -rf ./target/debug/openfang

cargo run --package openfang-cli start
