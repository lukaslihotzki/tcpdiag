#!/bin/sh -ex
cargo build --verbose
cargo +nightly build -p synconn
export PATH=$PATH:target/debug
CONNS=10
COUNT=10
synconn "$CONNS" tcpdiag -c"$COUNT" -p.1 -o binary --dport > data.bin
for fmt in json csv; do
    tcpdiag --convert -o "$fmt" < data.bin > data.$fmt
    tcpdiag --convert -o binary < data.$fmt > data$fmt.bin
    tcpdiag --convert -o "$fmt" < data$fmt.bin > data$fmt.$fmt
    cmp "data.$fmt" "data$fmt.$fmt"
done
test "$(wc -l <data.json)" = "$((COUNT))"
test "$(wc -l <data.csv)" = "$((1+CONNS*COUNT))"
