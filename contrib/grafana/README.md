# Grafana dashboard

`circus.json` is a Grafana dashboard for a Circus deployment. Import it from
**Dashboards → New → Import**, upload the file, and pick a Prometheus data
source when prompted.

## Prometheus

Everything comes from `circus-server`'s `/prometheus` endpoint, which is
unauthenticated and served from the same listener as the web UI.

```yaml
scrape_configs:
  - job_name: circus
    metrics_path: /prometheus
    static_configs:
      - targets: ["circus.example.com:3000"]
```

Every metric is a gauge recomputed from the database on each scrape, so the
scrape interval only controls dashboard resolution. One minute is plenty.

The `$job` and `$instance` variables scope the whole dashboard, and the panels
sum across whatever is selected. That is what you want when one Prometheus
scrapes several separate Circus deployments. If instead you run several
`circus-server` replicas against a single database, every replica reports the
same database-wide totals and the sums double, so pin `$instance` to one
replica.

## Loki

Panel 20 needs a Loki data source with journald logs, and asks for one under the
**Logs source** variable. The rest of the dashboard works without Loki. Delete
that panel and the `logs` variable if you do not ship logs.

The query matches the level token in the log line rather than parsing fields,
because the default `compact` tracing format prints `WARN` and `ERROR` as bare
tokens with no `level=` key for `logfmt` to find. If you set `format = "json"`
under `[tracing]` in the server config, the stricter

```logql
{unit=~"circus-.+"} | json | level=~"WARN|ERROR"
```

works and will not match the words inside a message body.
