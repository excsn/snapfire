# load

HTTP load against a running FSR server. Each worker of `loadgen` holds one HTTP/1.1 connection and sends the next request as soon as the last response's body is read. It opens a new connection only when its own one fails, so a clean run reports `connections` equal to its concurrency at any level, c=1 included. A pooled client opens extra connections when a request races the pool checkout. Those close into TIME_WAIT and at low concurrency exhaust the local ports.

```sh
cargo build --release -p load -p shopping_react_ts
./target/release/matrix shopping http://127.0.0.1:8080/ 20 1,16,64,256 -- ./target/release/shopping_react_ts
./target/release/loadgen http://127.0.0.1:8080/ 16 20     # one run against a server already up
```

`matrix` starts the server command afresh for every level, with `RELEASE_ENV=production` and a generated `C5_SESSION__KEY` unless either is already set. It times the first 200, runs the load and stops the server with SIGTERM, reading the server's peak RSS and CPU time from `wait4`. Levels are 8 seconds apart. The shopping binary also serves its two backends on 8081 and 8082, so those ports need to be free too.

| Field | What it is |
| --- | --- |
| `cold_ms` | spawn to the first 200 |
| `done`, `rps` | responses read in full; the same per second of the run |
| `p50_ms` … `max_ms` | latency from sending a request to having read its body |
| `non2xx`, `failed` | responses outside 2xx; requests that ended in a connection or body error |
| `connections` | TCP connections opened |
| `peak_rss_mb`, `cpu_pct` | the server's peak RSS; its CPU time over the run's wall time, 100 per core |

| File | What it is |
| --- | --- |
| `results/<label>.jsonl` | one line per level, appended each run |
| `results/<label>-serve.log` | the server's output |
