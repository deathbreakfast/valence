# Valence performance

Measured on AWS (`c6i.xlarge` class hosts unless noted). These rows are **debug builds** from a two-host campaign — comparative adapter ranking, not product SLOs. Full matrices come from AWS campaign runs.

## Write capacity

Single-client write firehose (bm-v5) on the measured hosts:

| Storage | ops/s |
|---------|-------|
| mem | ~99k |
| indradb | ~51k |
| redis | ~18k |
| sqlite | ~10k |
| postgres | ~2.7k |

Create+get op p95 (bm-v0) spans mem **0.008 ms** to postgres **~2.8 ms**. Under rising concurrency, keep error rate under about 0.1% when sizing client pools. Multi-client aggregate writes on Redis fleets are the right model when many app processes share one Valence deployment.

## Query and read paths

Hot get-by-id p95 (bm-v20 cache-off path): mem **0.002 ms**, redis **~0.27 ms**, postgres **~0.76 ms**. Compiled and ORM query latency grows with graph depth and filter shape. Hybrid IndraDB cache with Postgres primary improves hot get/query/hop mixes versus Postgres alone on the same hardware class.

## Privacy and overhead

Enabling privacy evaluation on reads adds measurable latency versus a forced bypass. Treat privacy-on as the production default when quoting capacity; bypass figures are upper bounds only.

## How to read these results

Prefer AWS-tagged hardware labels when comparing deployments. Developer-laptop runs are useful for harness smoke, not for fleet sizing. Release-build ceilings and multi-client saturation curves are still open campaign work in the lab study.
