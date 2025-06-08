# tcpdiag

tcpdiag is a tool to read TCP connection information from the Linux kernel. It
uses SOCK_DIAG netlink sockets internally.

Specify the `-4` or `-6` argument if you only need a single address family.
Otherwise, tcpdiag needs to issue two consecutive netlink requests, one for each
address family. Linux also offers port filtering, which is exposed using the
`--sport` and `--dport` arguments. By default, only established connections are
captured. Specify `--all-states` to capture connections in all states.
Furthermore, the `--all-extensions` argument can be used to request all types of
data from Linux. This only makes sense when using the binary output format.
For periodic capturing, specify the period length using `-p` (in seconds).
Optionally, the count of periods to be captured can be set with `-c`.

In addition to the INET_DIAG data, tcpdiag captures the timestamp on the start
of each measurement period and the duration of the active part of the
measurement period. This duration starts before sending the first netlink
request and ends after the reception of the last netlink response. The duration
depends on the number of connections and typically decreases when decreasing
the period length.

tcpdiag supports multiple output formats (binary, json, and csv). The output
format is selected using `-o`. By specifying the `--convert` argument,
an existing capture on stdin can be converted to another format. In this case,
the input format is detected automatically.

## Repository Structure

This repository is a workspace that also contains other crates. csv is a custom
crate to serialize data as csv. In contrast to the existing csv crate for
serde, it supports nested fields and static computation of the CSV header.
The synconn crate creates synthetic connections for tests and benchmarks. It is
used by `./test.sh`.

## License

This project is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
