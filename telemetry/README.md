# Local telemetry setup

To spawn a local telemetry stack with OpenTelemetry for telemetry collection, Prometheus for metrics, Loki for logs and
Grafana for visualization, run

```bash
docker compose -f telemetry/docker-compose.yml up
```

from the root of the repository.

Once the telemetry stack is up, start your local training setup as usual, but remember to add the OTLP arguments when
running the Psyche client:

```
OTLP_METRICS_URL = "http://localhost:4318/v1/metrics" # OpenTelemetry collector metrics endpoint
OTLP_LOGS_URL = "http://localhost:4318/v1/logs"       # OpenTelemetry collector logs endpoint
```

For convenience, you can run `just dev start-training-localnet-light-client-telemetry` to start the Psyche client with
the arguments already set for telemetry collection
